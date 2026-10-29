//! Explicitly opt-in diagnostic: prints response field TYPES, never values or credentials.
use serde_json::Value;
fn schema(v: &Value, depth: usize) -> Value {
    if depth == 0 {
        return Value::String("nested".into());
    }
    match v {
        Value::Object(o) => Value::Object(
            o.iter()
                .filter(|(k, _)| {
                    !matches!(
                        k.as_str(),
                        "raw" | "token" | "accessToken" | "refreshToken" | "credentials"
                    )
                })
                .map(|(k, v)| (k.clone(), schema(v, depth - 1)))
                .collect(),
        ),
        Value::Array(a) => {
            serde_json::json!({"arrayCount":a.len(),"item":a.first().map(|v|schema(v,depth-1))})
        }
        Value::String(_) => Value::String("string".into()),
        Value::Number(_) => Value::String("number".into()),
        Value::Bool(_) => Value::String("boolean".into()),
        Value::Null => Value::String("null".into()),
    }
}
#[tokio::test]
#[ignore = "requires an explicitly configured local Agent2API credential"]
async fn configured_service_field_types_only() {
    if std::env::var("TOKEN_GATEWAY_LIVE").as_deref() != Ok("1") {
        return;
    }
    let raw = keyring::Entry::new("TokenPlusPlus", "agent2api")
        .unwrap()
        .get_password()
        .expect("Save the management token in the app first");
    let bound: Value = serde_json::from_str(&raw).unwrap();
    let base = bound["base"].as_str().unwrap();
    let key = bound["key"].as_str().unwrap();
    let url = url::Url::parse(base).unwrap();
    assert!(
        url.scheme() == "https"
            || url.host_str() == Some("127.0.0.1")
            || url.host_str() == Some("localhost")
    );
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .unwrap();
    let response = client
        .get(format!("{base}/api/accounts/usage/snapshot"))
        .bearer_auth(key)
        .send()
        .await
        .expect("Connection failed");
    assert!(response.status().is_success());
    let root: Value = response.json().await.unwrap();
    let data = root.get("data").unwrap_or(&root);
    let accounts: Value = client
        .get(format!("{base}/api/accounts"))
        .bearer_auth(key)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let accounts = accounts.get("data").unwrap_or(&accounts);
    for (index, row) in data["results"].as_array().unwrap().iter().enumerate() {
        let id = row["id"].as_str().unwrap();
        let provider = accounts["accounts"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["id"] == row["id"])
            .and_then(|a| a["provider"].as_str())
            .unwrap_or("unknown");
        println!(
            "account index {index} provider {provider} cached usage schema: {}",
            schema(&row["usage"], 4)
        );
        let fresh: Value = client
            .get(format!("{base}/api/accounts/usage"))
            .query(&[("id", id)])
            .bearer_auth(key)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        let fresh = fresh.get("data").unwrap_or(&fresh);
        println!(
            "provider {provider} fresh response schema: {}",
            schema(fresh, 5)
        );
    }
}
