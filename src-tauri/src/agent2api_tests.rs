//! Synthetic loopback fixtures. No keyring entries or real service requests.
use super::*;
use serde_json::json;
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{atomic::AtomicUsize, Mutex},
    thread,
};
type Handler = dyn Fn(&str) -> (u16, String, String) + Send + Sync;
struct Server {
    base: String,
    stop: Arc<AtomicBool>,
    hits: Arc<AtomicUsize>,
    worker: Option<thread::JoinHandle<()>>,
}
impl Server {
    fn new(handler: Arc<Handler>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let stop = Arc::new(AtomicBool::new(false));
        let signal = stop.clone();
        let hits = Arc::new(AtomicUsize::new(0));
        let count = hits.clone();
        let worker = thread::spawn(move || {
            while !signal.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let handler = handler.clone();
                        count.fetch_add(1, Ordering::Relaxed);
                        thread::spawn(move || {
                            // Accepted sockets may inherit nonblocking mode on Windows.
                            stream.set_nonblocking(false).unwrap();
                            stream
                                .set_read_timeout(Some(Duration::from_secs(3)))
                                .unwrap();
                            let mut buffer = [0; 16384];
                            let mut n = 0;
                            while n < buffer.len()
                                && !buffer[..n].windows(4).any(|w| w == b"\r\n\r\n")
                            {
                                match stream.read(&mut buffer[n..]) {
                                    Ok(0) | Err(_) => return,
                                    Ok(size) => n += size,
                                }
                            }
                            let request = String::from_utf8_lossy(&buffer[..n]);
                            let (status, body, headers) = handler(&request);
                            let _=write!(stream,"HTTP/1.1 {status} Fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n{headers}\r\n{body}",body.len());
                        });
                    }
                    Err(_) => thread::sleep(Duration::from_millis(3)),
                }
            }
        });
        Self {
            base,
            stop,
            hits,
            worker: Some(worker),
        }
    }
    fn connection(&self) -> Connection {
        Connection {
            base: self.base.clone(),
            client: reqwest::Client::builder()
                .no_proxy()
                .redirect(reqwest::redirect::Policy::none())
                .timeout(Duration::from_secs(4))
                .build()
                .unwrap(),
            key: "TokenPlusPlus-isolated-fixture".into(),
            blocked: Arc::new(AtomicBool::new(false)),
        }
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        let _ = TcpStream::connect(self.base.trim_start_matches("http://"));
        if let Some(w) = self.worker.take() {
            let _ = w.join();
        }
    }
}
#[tokio::test]
async fn management_401_stops_further_requests_and_never_exposes_body() {
    let server = Server::new(Arc::new(|_| {
        (
            401,
            "{\"secret\":\"fixture-must-not-reach-ui\"}".into(),
            String::new(),
        )
    }));
    let c = server.connection();
    let error = c.get("/api/accounts", None).await.unwrap_err();
    assert!(error.contains("令牌失效"));
    assert!(!error.contains("fixture-must"));
    assert!(c.get("/api/models/manage", None).await.is_err());
    assert_eq!(server.hits.load(Ordering::Relaxed), 1);
}
#[tokio::test]
async fn redirects_never_forward_management_credentials() {
    let target = Server::new(Arc::new(|_| (200, "{}".into(), String::new())));
    let location = target.base.clone();
    let source = Server::new(Arc::new(move |_| {
        (
            302,
            String::new(),
            format!("Location: {location}/api/accounts\r\n"),
        )
    }));
    let error = source
        .connection()
        .get("/api/accounts", None)
        .await
        .unwrap_err();
    assert!(error.contains("重定向"));
    assert_eq!(target.hits.load(Ordering::Relaxed), 0);
}
#[tokio::test]
async fn unsupported_schema_http_errors_and_limits_are_explicit() {
    for (code, text) in [(404, "版本"), (403, "权限"), (429, "限流")] {
        let server = Server::new(Arc::new(move |_| {
            (code, "{}".into(), "Retry-After: 1\r\n".into())
        }));
        assert!(server
            .connection()
            .get("/api/accounts", None)
            .await
            .unwrap_err()
            .contains(text));
    }
    assert!(catalog(&json!({"other":[]}), &json!({"models":[]})).is_err());
}
#[tokio::test]
async fn selected_accounts_only_four_parallel_and_partial_failure_isolated() {
    let paths = Arc::new(Mutex::new(Vec::new()));
    let seen = paths.clone();
    let active = Arc::new(AtomicUsize::new(0));
    let inflight = active.clone();
    let max = Arc::new(AtomicUsize::new(0));
    let peak = max.clone();
    let now = chrono::Utc::now().timestamp();
    let server = Server::new(Arc::new(move |req| {
        assert!(req.to_lowercase().contains(&format!(
            "authorization: bearer {}",
            "tokenplusplus-isolated-fixture"
        )));
        let path = req.split_whitespace().nth(1).unwrap();
        seen.lock().unwrap().push(path.to_string());
        let value = match path {
            "/api/accounts" => {
                json!({"accounts":(0..7).map(|i|json!({"id":format!("a{i}"),"name":format!("示例{i}"),"provider":"qoder","enabled":i!=6,"token":"fixture-must-not-reach-ui"})).collect::<Vec<_>>()})
            }
            "/api/models/manage" => json!({"models":[{"id":"glm-4.6","provider":"qoder"}]}),
            "/api/accounts/usage/snapshot" => json!({"at":now*1000,"results":[]}),
            _ => {
                assert!(path.starts_with("/api/accounts/usage?id=a"));
                let count = inflight.fetch_add(1, Ordering::Relaxed) + 1;
                peak.fetch_max(count, Ordering::Relaxed);
                thread::sleep(Duration::from_millis(80));
                inflight.fetch_sub(1, Ordering::Relaxed);
                let id = path.split('=').nth(1).unwrap();
                if id == "a5" {
                    json!({"results":[{"id":id,"error":"fixture-must-not-reach-ui","code":401}]})
                } else {
                    json!({"results":[{"id":id,"error":null,"usage":{"available":"123.45","unit":"CNY","raw":{"token":"fixture-must-not-reach-ui"}}}]})
                }
            }
        };
        (
            200,
            json!({"success":true,"data":value}).to_string(),
            String::new(),
        )
    }));
    let settings = Settings {
        agent2api_url: server.base.clone(),
        ..Default::default()
    };
    let snapshot = query_connection(&settings, server.connection())
        .await
        .unwrap();
    assert_eq!(snapshot.status, "stale");
    let safe = serde_json::to_string(&snapshot).unwrap();
    assert!(!safe.contains("fixture-must-not-reach-ui"));
    let data = snapshot.gateway.unwrap();
    assert_eq!(data.accounts[0].metrics[0].metric.value, "123.45");
    assert_eq!(data.accounts[5].status, "error");
    assert!(!paths.lock().unwrap().iter().any(|p| p.contains("id=a6")));
    assert!((2..=4).contains(&max.load(Ordering::Relaxed)));
}
