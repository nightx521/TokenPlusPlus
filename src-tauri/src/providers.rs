use crate::model::{Detail, Metric, Settings, Snapshot};
use chrono::{DateTime, Utc};
use hmac::{Hmac, Mac};
use reqwest::Client;
use rust_decimal::Decimal;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashSet},
    path::PathBuf,
    str::FromStr,
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::Command,
};

static RATE_LIMITS: std::sync::OnceLock<std::sync::Mutex<BTreeMap<String, i64>>> =
    std::sync::OnceLock::new();
pub fn cooldown_seconds(provider: &str) -> u64 {
    RATE_LIMITS
        .get_or_init(Default::default)
        .lock()
        .ok()
        .and_then(|limits| limits.get(provider).copied())
        .map_or(0, |until| {
            until.saturating_sub(Utc::now().timestamp()).max(0) as u64
        })
}

pub fn record_retry_after(provider: &str, value: Option<&str>) {
    let now = Utc::now().timestamp();
    let until = value
        .and_then(|s| {
            s.parse::<i64>()
                .ok()
                .filter(|n| *n >= 0)
                .map(|n| now.saturating_add(n))
                .or_else(|| DateTime::parse_from_rfc2822(s).ok().map(|d| d.timestamp()))
        })
        .unwrap_or(now + 120);
    if let Ok(mut limits) = RATE_LIMITS.get_or_init(Default::default).lock() {
        limits.insert(provider.into(), until.max(now + 1));
    }
}

pub fn text(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}
fn decimal(v: &Value) -> Result<Decimal, String> {
    let n = text(v)
        .and_then(|s| Decimal::from_str(&s).ok())
        .ok_or("平台未返回有效余额，暂时无法统计")?;
    if n < Decimal::ZERO {
        return Err("平台返回负数余额，请到控制台核对".into());
    }
    Ok(n)
}
fn currency(v: &Value) -> Result<String, String> {
    let c = v.as_str().ok_or("平台未返回币种")?;
    if !["CNY", "USD"].contains(&c) {
        return Err("暂不支持平台返回的币种".into());
    }
    Ok(c.into())
}
fn money(label: &str, n: Decimal, unit: &str) -> Metric {
    Metric {
        label: label.into(),
        value: n.round_dp(2).to_string(),
        unit: unit.into(),
        remaining_percent: None,
        resets_at: None,
    }
}
pub fn credential(provider: &str) -> Result<String, String> {
    keyring::Entry::new("TokenPlusPlus", provider)
        .map_err(|_| "无法访问 Windows 凭据管理器".to_string())?
        .get_password()
        .map_err(|e| match e {
            keyring::Error::NoEntry => "请先在设置中配置凭据".into(),
            _ => "无法读取系统中保存的凭据".into(),
        })
}
async fn body(response: reqwest::Response) -> Result<Value, String> {
    match response.status().as_u16() {
        200..=299 => response.json().await.map_err(|_| "平台响应无法解析".into()),
        401 => Err("凭据无效或登录已过期，请重新配置或登录".into()),
        403 => Err("没有查询权限，请检查凭据及账务只读授权".into()),
        429 => {
            let provider = match response.url().host_str().unwrap_or("") {
                "api.deepseek.com" => "deepseek",
                _ => "huawei",
            };
            let now = Utc::now().timestamp();
            let until = response
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| {
                    s.parse::<i64>()
                        .ok()
                        .filter(|n| *n >= 0)
                        .map(|n| now.saturating_add(n))
                        .or_else(|| DateTime::parse_from_rfc2822(s).ok().map(|d| d.timestamp()))
                })
                .unwrap_or(now + 120);
            if let Ok(mut limits) = RATE_LIMITS.get_or_init(Default::default).lock() {
                limits.insert(provider.into(), until.max(now + 1));
            }
            Err("平台查询限流，按服务端等待时间自动重试".into())
        }
        n => Err(format!("平台返回 HTTP {n}，请稍后重试")),
    }
}
fn network(_: reqwest::Error) -> String {
    "连接平台失败，请检查网络或代理设置".into()
}

pub fn parse_deepseek(v: &Value) -> Result<Snapshot, String> {
    let infos = v["balance_infos"]
        .as_array()
        .filter(|a| !a.is_empty())
        .ok_or("平台未返回余额明细")?;
    let mut s = Snapshot::success("deepseek");
    for info in infos {
        let unit = currency(&info["currency"])?;
        s.metrics
            .push(money("可用余额", decimal(&info["total_balance"])?, &unit));
        for (key, name) in [
            ("topped_up_balance", "充值余额"),
            ("granted_balance", "赠金余额"),
        ] {
            s.details.push(Detail {
                label: name.into(),
                value: format!("{} {}", decimal(&info[key])?.round_dp(2), unit),
            });
        }
    }
    Ok(s)
}
pub async fn deepseek(client: &Client) -> Result<Snapshot, String> {
    let key = credential("deepseek")?;
    let v = body(
        client
            .get("https://api.deepseek.com/user/balance")
            .bearer_auth(key)
            .send()
            .await
            .map_err(network)?,
    )
    .await?;
    parse_deepseek(&v)
}

pub fn huawei_authorization(ak: &str, sk: &str, host: &str, query: &str, date: &str) -> String {
    let payload = hex::encode(Sha256::digest(b""));
    let canonical = format!("GET\n/v2/promotions/benefits/coupons/\n{query}\nhost:{host}\nx-sdk-date:{date}\n\nhost;x-sdk-date\n{payload}");
    let to_sign = format!(
        "SDK-HMAC-SHA256\n{date}\n{}",
        hex::encode(Sha256::digest(canonical.as_bytes()))
    );
    let mut mac =
        Hmac::<Sha256>::new_from_slice(sk.as_bytes()).expect("HMAC accepts any key length");
    mac.update(to_sign.as_bytes());
    format!(
        "SDK-HMAC-SHA256 Access={ak}, SignedHeaders=host;x-sdk-date, Signature={}",
        hex::encode(mac.finalize().into_bytes())
    )
}
fn timestamp(v: &Value) -> Option<i64> {
    v.as_str()
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|d| d.timestamp())
}

pub fn parse_coupons(coupons: &[Value], now: i64) -> Result<Snapshot, String> {
    let mut seen = HashSet::new();
    let mut totals: BTreeMap<String, Decimal> = BTreeMap::new();
    let mut details = vec![];
    let mut active = 0;
    let mut nearest: Option<i64> = None;
    for c in coupons {
        if text(&c["coupon_type"]).as_deref() != Some("1") {
            continue;
        }
        let id = c["coupon_id"]
            .as_str()
            .filter(|id| !id.is_empty())
            .ok_or("代金券缺少标识，无法可靠去重")?;
        if !seen.insert(id) {
            continue;
        }
        let status = text(&c["status"]).ok_or("代金券缺少状态，无法确认可用性")?;
        if matches!(status.as_str(), "4" | "5") {
            continue;
        }
        if !matches!(status.as_str(), "2" | "3") {
            return Err("发现未知代金券状态，请到控制台核对".into());
        }
        let start = timestamp(&c["valid_time"]).ok_or("代金券缺少有效生效时间")?;
        let end = timestamp(&c["expire_time"]).ok_or("代金券缺少有效到期时间")?;
        if start > now || end <= now {
            continue;
        }
        match text(&c["is_frozen"]).as_deref() {
            Some("1") => continue,
            Some("0") => {}
            _ => return Err("代金券缺少冻结状态，无法确认可用性".into()),
        }
        let n = decimal(&c["balance"])?;
        if n == Decimal::ZERO {
            continue;
        }
        if status == "3" && text(&c["coupon_version"]).as_deref() != Some("2") {
            continue;
        }
        let unit = currency(&c["currency"])?;
        *totals.entry(unit.clone()).or_default() += n;
        active += 1;
        nearest = Some(nearest.map_or(end, |old| old.min(end)));
        let name = c["plan_name"]
            .as_str()
            .filter(|s| !s.is_empty())
            .unwrap_or("代金券");
        let expiry = DateTime::from_timestamp(end, 0)
            .unwrap()
            .with_timezone(&chrono::FixedOffset::east_opt(8 * 3600).unwrap())
            .format("%m/%d %H:%M")
            .to_string();
        details.push(Detail {
            label: name.chars().take(60).collect(),
            value: format!("{} {} · {} 到期", n.round_dp(2), unit, expiry),
        });
    }
    let mut s = Snapshot::success("huawei");
    // An empty, complete response proves zero; missing fields never do.
    if totals.is_empty() {
        totals.insert("CNY".into(), Decimal::ZERO);
    }
    for (unit, n) in totals {
        let mut m = money("代金券余额", n, &unit);
        m.resets_at = nearest;
        s.metrics.push(m);
    }
    s.message = format!("{active} 张可用 · 按全部可用代金券统计");
    s.details = details;
    s.details.push(Detail {
        label: "统计口径".into(),
        value: "全部可用代金券，未筛选 MaaS 适用范围".into(),
    });
    Ok(s)
}

pub async fn huawei(client: &Client, settings: &Settings) -> Result<Snapshot, String> {
    let saved: Value =
        serde_json::from_str(&credential("huawei")?).map_err(|_| "凭据格式异常，请重新保存")?;
    let ak = saved["key"].as_str().ok_or("请配置 AK")?;
    let sk = saved["secret"].as_str().ok_or("请配置 SK")?;
    let host = if settings.huawei_site == "intl" {
        "bss-intl.myhuaweicloud.com"
    } else {
        "bss.myhuaweicloud.com"
    };
    let mut coupons = vec![];
    for page in 0..100 {
        let query = format!("limit=100&offset={}", page * 100);
        let date = Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
        let response = client
            .get(format!(
                "https://{host}/v2/promotions/benefits/coupons?{query}"
            ))
            .header("host", host)
            .header("x-sdk-date", &date)
            .header(
                "authorization",
                huawei_authorization(ak, sk, host, &query, &date),
            )
            .send()
            .await
            .map_err(network)?;
        let v = body(response).await?;
        if !v["error_code"].is_null() {
            return Err("华为云查询未成功，请检查账务权限与站点".into());
        }
        let rows = v["user_coupons"]
            .as_array()
            .ok_or("华为云未返回优惠券列表")?;
        let count = v["count"]
            .as_u64()
            .ok_or("华为云未返回列表总数，无法确认统计完整")?;
        coupons.extend(rows.iter().cloned());
        if coupons.len() as u64 >= count {
            return parse_coupons(&coupons, Utc::now().timestamp());
        }
        if rows.is_empty() {
            return Err("华为云分页数据不完整，请重新刷新".into());
        }
    }
    Err("优惠券数量超出首版查询上限，请到控制台查看".into())
}

pub fn parse_codex(v: &Value) -> Result<Snapshot, String> {
    let mut s = Snapshot::success("codex");
    let buckets: Vec<(String, &Value)> = match v["rateLimitsByLimitId"]
        .as_object()
        .filter(|m| !m.is_empty())
    {
        Some(map) => map.iter().map(|(k, v)| (k.clone(), v)).collect(),
        None if v["rateLimits"].is_object() => vec![("Codex".into(), &v["rateLimits"])],
        _ => return Err("当前账号未返回订阅额度，请确认已使用 ChatGPT 账号登录".into()),
    };
    for (id, bucket) in buckets {
        for key in ["primary", "secondary"] {
            let window = &bucket[key];
            if window.is_null() {
                continue;
            }
            let used = window["usedPercent"]
                .as_f64()
                .filter(|n| n.is_finite())
                .ok_or("平台未返回有效的额度百分比")?;
            let minutes = window["windowDurationMins"].as_u64();
            let name = bucket["limitName"].as_str().unwrap_or(&id);
            let duration = match minutes {
                Some(10080) => "每周".into(),
                Some(m) if m % 60 == 0 => format!("{} 小时", m / 60),
                Some(m) => format!("{m} 分钟"),
                None => {
                    if key == "primary" {
                        "主额度".into()
                    } else {
                        "次额度".into()
                    }
                }
            };
            let remaining = (100.0 - used).clamp(0.0, 100.0);
            s.metrics.push(Metric {
                label: format!("{name} · {duration}"),
                value: format!("{remaining:.0}"),
                unit: "%".into(),
                remaining_percent: Some(remaining),
                resets_at: window["resetsAt"].as_i64().filter(|t| *t > 0),
            });
        }
    }
    if s.metrics.is_empty() {
        return Err("当前账号没有可显示的额度窗口".into());
    }
    Ok(s)
}

fn codex_executable(custom: &str) -> Result<PathBuf, String> {
    if !custom.trim().is_empty() {
        let p = PathBuf::from(custom.trim());
        if p.is_file() && p.extension().is_some_and(|e| e.eq_ignore_ascii_case("exe")) {
            return Ok(p);
        }
        return Err("Codex 路径无效，请选择 codex.exe".into());
    }
    if let Some(paths) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&paths) {
            let p = dir.join("codex.exe");
            if p.is_file() {
                return Ok(p);
            }
        }
    }
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        let root = PathBuf::from(local).join("OpenAI/Codex/bin");
        if let Ok(entries) = std::fs::read_dir(root) {
            let mut paths: Vec<_> = entries
                .flatten()
                .map(|e| e.path().join("codex.exe"))
                .filter(|p| p.is_file())
                .collect();
            paths.sort_by_key(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok());
            if let Some(p) = paths.pop() {
                return Ok(p);
            }
        }
    }
    Err("未找到 Codex，请在设置中填写 codex.exe 的完整路径".into())
}

pub async fn codex(settings: &Settings) -> Result<Snapshot, String> {
    let path = codex_executable(&settings.codex_path)?;
    let mut command = Command::new(path);
    command
        .args(["app-server", "--stdio"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(0x08000000);
    let mut child = command
        .spawn()
        .map_err(|_| "无法启动 Codex 额度查询，请检查安装")?;
    let mut stdin = child.stdin.take().ok_or("无法连接 Codex")?;
    let stdout = child.stdout.take().ok_or("无法读取 Codex")?;
    let mut lines = BufReader::new(stdout).lines();
    let query = async {
        let init = json!({"id":1,"method":"initialize","params":{"clientInfo":{"name":"token_plusplus","title":"Token++","version":env!("CARGO_PKG_VERSION")},"capabilities":{"experimentalApi":true}}});
        stdin
            .write_all(format!("{init}\n").as_bytes())
            .await
            .map_err(|_| "Codex 连接中断")?;
        loop {
            let line = lines
                .next_line()
                .await
                .map_err(|_| "无法读取 Codex 响应")?
                .ok_or("Codex 查询已退出")?;
            let v: Value = serde_json::from_str(&line).map_err(|_| "Codex 响应格式异常")?;
            if v["id"] == 1 {
                if !v["error"].is_null() {
                    return Err("Codex 初始化失败，请更新客户端".into());
                }
                break;
            }
        }
        let message =
            "{\"method\":\"initialized\"}\n{\"id\":2,\"method\":\"account/rateLimits/read\"}\n";
        stdin
            .write_all(message.as_bytes())
            .await
            .map_err(|_| "Codex 连接中断")?;
        loop {
            let line = lines
                .next_line()
                .await
                .map_err(|_| "无法读取 Codex 响应")?
                .ok_or("Codex 查询已退出")?;
            let v: Value = serde_json::from_str(&line).map_err(|_| "Codex 响应格式异常")?;
            if v["id"] == 2 {
                if !v["error"].is_null() {
                    return Err("Codex 未能读取额度，请在官方客户端确认已登录并稍后重试".into());
                }
                return parse_codex(&v["result"]);
            }
        }
    };
    let result = tokio::time::timeout(Duration::from_secs(30), query)
        .await
        .map_err(|_| "Codex 查询超时，请确认官方客户端已登录".to_string())?;
    let _ = child.kill().await;
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    fn coupon(id: &str, balance: &str) -> Value {
        json!({"coupon_id":id,"coupon_type":1,"status":3,"coupon_version":2,"valid_time":"2020-01-01T00:00:00Z","expire_time":"2030-01-01T00:00:00Z","is_frozen":"0","balance":balance,"currency":"CNY"})
    }
    #[test]
    fn sums_remaining_not_face_value_and_deduplicates() {
        let mut a = coupon("a", "0.10");
        a["face_value"] = json!(100);
        let s = parse_coupons(&[a.clone(), coupon("b", "0.20"), a], 1800000000).unwrap();
        assert_eq!(s.metrics[0].value, "0.30");
    }
    #[test]
    fn expired_frozen_future_and_spent_are_excluded() {
        let mut frozen = coupon("a", "10");
        frozen["is_frozen"] = json!("1");
        let mut expired = coupon("b", "10");
        expired["expire_time"] = json!("2021-01-01T00:00:00Z");
        let mut future = coupon("c", "10");
        future["valid_time"] = json!("2029-01-01T00:00:00Z");
        let s = parse_coupons(&[frozen, expired, future, coupon("d", "0")], 1800000000).unwrap();
        assert_eq!(s.metrics[0].value, "0");
    }
    #[test]
    fn missing_balance_is_unknown_not_zero() {
        let mut c = coupon("a", "1");
        c["balance"] = Value::Null;
        assert!(parse_coupons(&[c], 1800000000).is_err());
        assert!(parse_deepseek(&json!({"balance_infos":[]})).is_err());
    }
    #[test]
    fn currencies_are_kept_separate() {
        let mut usd = coupon("b", "3");
        usd["currency"] = json!("USD");
        assert_eq!(
            parse_coupons(&[coupon("a", "5"), usd], 1800000000)
                .unwrap()
                .metrics
                .len(),
            2
        );
    }
    #[test]
    fn codex_uses_buckets_and_seconds() {
        let s = parse_codex(&json!({"rateLimitsByLimitId":{"codex":{"primary":{"usedPercent":23,"windowDurationMins":300,"resetsAt":1800000000},"secondary":{"usedPercent":104,"windowDurationMins":10080}}}})).unwrap();
        assert_eq!(s.metrics[0].remaining_percent, Some(77.0));
        assert_eq!(s.metrics[0].resets_at, Some(1800000000));
        assert_eq!(s.metrics[1].remaining_percent, Some(0.0));
        assert_eq!(s.metrics[1].resets_at, None);
    }
    #[tokio::test]
    #[ignore = "requires locally logged-in official Codex"]
    async fn live_codex() {
        let s = codex(&Settings::default()).await.expect("local Codex read");
        assert!(!s.metrics.is_empty());
        println!("Codex live check: {} quota windows", s.metrics.len());
    }

    #[test]
    #[ignore = "writes and removes an isolated Windows credential self-test entry"]
    fn windows_credential_roundtrip() {
        let name = format!(
            "self-test-{}-{}",
            std::process::id(),
            Utc::now().timestamp_millis()
        );
        let entry = keyring::Entry::new("TokenPlusPlus.SelfTest", &name).unwrap();
        entry.set_password("local-inert-test-value").unwrap();
        let retrieved = entry.get_password();
        entry.delete_credential().unwrap();
        assert_eq!(retrieved.unwrap(), "local-inert-test-value");
        assert!(matches!(entry.get_password(), Err(keyring::Error::NoEntry)));
    }
}
