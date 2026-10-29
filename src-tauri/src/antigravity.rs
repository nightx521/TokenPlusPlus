use crate::{
    model::{Detail, Metric, Settings, Snapshot},
    providers,
};
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;
use std::{collections::HashSet, time::Duration};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountChoice {
    pub id: String,
    pub label: String,
    pub available: bool,
    pub groups: Vec<String>,
}

pub fn validate(settings: &Settings) -> Result<(), String> {
    service_base(settings)?;
    if settings.antigravity_port == 0 || !(1..=120).contains(&settings.antigravity_minutes) {
        return Err("Tools 端口或刷新间隔无效".into());
    }
    if settings.antigravity_account.len() > 128
        || !settings
            .antigravity_account
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
    {
        return Err("请选择有效的 Tools 账号".into());
    }
    if settings.antigravity_groups.len() > 64
        || settings.antigravity_groups.iter().any(|g| g.len() > 256)
    {
        return Err("选择的额度池无效".into());
    }
    Ok(())
}

/// Service root (including an optional reverse-proxy prefix), without /api.
pub fn service_base(settings: &Settings) -> Result<String, String> {
    normalize_address(
        if settings.antigravity_url.is_empty() {
            format!("http://127.0.0.1:{}", settings.antigravity_port)
        } else {
            settings.antigravity_url.trim().to_owned()
        }
        .as_str(),
    )
}

pub fn view(settings: &Settings) -> crate::model::QuotaView {
    crate::model::QuotaView {
        account: settings.antigravity_account.clone(),
        groups: settings.antigravity_groups.clone(),
        service: service_base(settings).unwrap_or_default(),
    }
}

pub fn retain_view(previous: Option<&Snapshot>, old: &Settings) -> Snapshot {
    if let Some(previous) = previous.filter(|s| !s.metrics.is_empty()) {
        let mut snapshot = previous.clone();
        if snapshot.view.is_none() {
            snapshot.view = Some(view(old));
        }
        snapshot.status = "stale".into();
        snapshot.message = "显示选项已更新，暂显示上次结果，等待刷新".into();
        snapshot
    } else {
        Snapshot::empty("antigravity", "idle", "等待查询新视图")
    }
}

fn selection_family(name: &str) -> Option<&'static str> {
    let key: String = name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect();
    match key.as_str() {
        "gemini" | "geminimodels" => Some("gemini"),
        "claude" | "gpt" | "claudemodels" | "claudegpt" | "claudegptmodels" => Some("claude_gpt"),
        _ => None,
    }
}
fn matches_selection(selected: &str, name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    match selection_family(selected) {
        Some("gemini") => lower.contains("gemini"),
        Some("claude_gpt") => lower.contains("claude") || lower.contains("gpt"),
        _ => selected == name,
    }
}

pub fn normalize_address(address: &str) -> Result<String, String> {
    let address = address.trim();
    if address.is_empty()
        || address.len() > 2048
        || address.contains('\\')
        || address.chars().any(char::is_control)
    {
        return Err("请填写有效的 Tools 服务地址".into());
    }
    let input = if address.contains("://") {
        address.to_owned()
    } else {
        format!("http://{address}")
    };
    let url = reqwest::Url::parse(&input)
        .map_err(|_| "Tools 地址格式无效，例如 http://localhost:8045")?;
    if !["http", "https"].contains(&url.scheme())
        || url.host().is_none()
        || url.port_or_known_default() == Some(0)
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(
            "地址只能包含 HTTP/HTTPS 服务地址和部署路径，不能包含密码、查询参数或片段".into(),
        );
    }
    let loopback = match url.host() {
        Some(url::Host::Domain(host)) => host.eq_ignore_ascii_case("localhost"),
        Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
        Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
        None => false,
    };
    if url.scheme() == "http" && !loopback {
        return Err(
            "云端地址请使用 https://；HTTP 仅限 localhost 或回环地址，可通过 SSH 隧道连接".into(),
        );
    }
    let path = url.path().trim_end_matches('/');
    if path.split('/').any(|part| part == "v1") {
        return Err("请填写 Tools 管理服务地址，不要填写 /v1 模型中转地址".into());
    }
    let base = url.as_str().trim_end_matches('/');
    Ok(base.strip_suffix("/api").unwrap_or(base).to_owned())
}

#[derive(serde::Serialize, serde::Deserialize)]
struct BoundCredential {
    format: String,
    base: String,
    key: String,
}
fn encode_key(settings: &Settings, key: &str) -> Result<String, String> {
    serde_json::to_string(&BoundCredential {
        format: "tools-endpoint-v1".into(),
        base: service_base(settings)?,
        key: key.trim().into(),
    })
    .map_err(|_| "Tools 凭据保存失败".into())
}
fn decode_key(settings: &Settings, value: &str) -> Result<String, String> {
    let bound: BoundCredential = serde_json::from_str(value)
        .map_err(|_| "Tools 凭据需要重新保存，请在连接设置中填写管理密钥")?;
    if bound.format != "tools-endpoint-v1"
        || bound.base != service_base(settings)?
        || bound.key.trim().is_empty()
    {
        return Err("服务地址已变化，请为此地址重新填写 Tools 管理密钥".into());
    }
    Ok(bound.key)
}
pub fn credential(settings: &Settings) -> Result<String, String> {
    decode_key(settings, &providers::credential("antigravity")?)
}
pub fn save_credential(settings: &Settings, key: &str) -> Result<(), String> {
    keyring::Entry::new("TokenPlusPlus", "antigravity")
        .map_err(|_| "无法访问系统凭据存储")?
        .set_password(&encode_key(settings, key)?)
        .map_err(|_| "Tools 凭据保存失败，请检查系统凭据管理器".into())
}
pub fn migrate_credential(settings: &Settings) {
    // Legacy keys were only used with the original loopback port. Never infer a remote destination.
    if settings.antigravity_url.is_empty() {
        if let Ok(value) = providers::credential("antigravity") {
            if serde_json::from_str::<BoundCredential>(&value).is_err() && !value.trim().is_empty()
            {
                let _ = save_credential(settings, &value);
            }
        }
    }
}

#[derive(Clone)]
struct Connection {
    client: reqwest::Client,
    base: String,
    key: String,
}
impl Connection {
    fn new(settings: &Settings) -> Result<Self, String> {
        validate(settings)?;
        if providers::cooldown_seconds("antigravity") > 0 {
            return Err("Tools 查询限流，请稍后重试".into());
        }
        Ok(Self {
            client: reqwest::Client::builder()
                .no_proxy()
                .redirect(reqwest::redirect::Policy::none())
                .timeout(Duration::from_secs(35))
                .build()
                .map_err(|_| "无法建立本机连接")?,
            base: format!("{}/api", service_base(settings)?),
            key: credential(settings)?,
        })
    }
    async fn get(&self, path: &str) -> Result<Value, String> {
        let mut response = self
            .client
            .get(format!("{}{path}", self.base))
            .bearer_auth(&self.key)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    "Tools 连接超时，请检查服务地址和网络"
                } else {
                    "无法连接 Tools，请检查地址、服务状态及 HTTPS 证书"
                }
            })?;
        match response.status().as_u16() {
            200 => {}
            401 => return Err("Tools 管理密钥无效；有独立管理密码时应填写管理密码".into()),
            403 => return Err("Tools 或账号拒绝访问，请在 Tools 中检查账号状态".into()),
            404 => return Err("该地址没有 Tools 额度管理接口，请检查地址和反向代理路径；/v1 中转接口不能查询账号额度".into()),
            429 => {
                providers::record_retry_after(
                    "antigravity",
                    response
                        .headers()
                        .get("retry-after")
                        .and_then(|v| v.to_str().ok()),
                );
                return Err("Tools 查询限流，按服务端等待时间自动重试".into());
            }
            _ => return Err("Tools 额度查询失败，请在 Tools 中检查连接与版本".into()),
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| "Tools 响应读取失败")? {
            if bytes.len() + chunk.len() > 2 * 1024 * 1024 {
                return Err("Tools 响应过大，请减少账号数量".into());
            }
            bytes.extend_from_slice(&chunk);
        }
        serde_json::from_slice(&bytes).map_err(|_| "Tools 返回格式不兼容，请检查版本".into())
    }
    async fn accounts(&self) -> Result<Vec<Value>, String> {
        let data = self.get("/accounts").await?;
        data.get("accounts")
            .and_then(Value::as_array)
            .cloned()
            .ok_or_else(|| "Tools 未返回账号列表".into())
    }
}
fn available(account: &Value) -> bool {
    !["disabled", "proxy_disabled", "validation_blocked"]
        .iter()
        .any(|field| account[*field].as_bool() == Some(true))
        && account["quota"]["is_forbidden"].as_bool() != Some(true)
}
fn label(account: &Value) -> String {
    account["name"]
        .as_str()
        .filter(|s| !s.is_empty())
        .unwrap_or("Tools 账号")
        .chars()
        .take(60)
        .collect()
}
pub async fn accounts(settings: &Settings) -> Result<Vec<AccountChoice>, String> {
    let connection = Connection::new(settings)?;
    Ok(connection
        .accounts()
        .await?
        .iter()
        .map(|a| AccountChoice {
            id: a["id"].as_str().unwrap_or_default().into(),
            label: label(a),
            available: available(a),
            groups: a["quota"]["quota_groups"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|g| g["display_name"].as_str().map(str::to_string))
                .collect(),
        })
        .filter(|a| !a.id.is_empty())
        .collect())
}

pub async fn query(settings: &Settings, force: bool) -> Result<Snapshot, String> {
    let connection = Connection::new(settings)?;
    let list = connection.accounts().await?;
    if settings.antigravity_account == "__all__" {
        let mut seen = HashSet::new();
        let accounts: Vec<_> = list
            .into_iter()
            .filter(available)
            .filter(|a| seen.insert(a["id"].as_str().unwrap_or_default().to_owned()))
            .collect();
        if accounts.is_empty() {
            return Err("Tools 中没有可查询的账号".into());
        }
        if accounts.len() > 64 {
            return Err("综合视图最多查询 64 个账号，请减少参与账号".into());
        }
        let expected = accounts.len();
        let mut jobs = tokio::task::JoinSet::new();
        let slots = std::sync::Arc::new(tokio::sync::Semaphore::new(4));
        for account in accounts {
            let connection = connection.clone();
            let mut settings = settings.clone();
            settings.antigravity_account = account["id"].as_str().unwrap_or_default().into();
            let slots = slots.clone();
            jobs.spawn(async move {
                let _permit = slots
                    .acquire_owned()
                    .await
                    .map_err(|_| "Tools 查询已取消".to_string())?;
                validate(&settings)?;
                query_account(&connection, &account, &settings, force).await
            });
        }
        let mut snapshots = Vec::new();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(60);
        while !jobs.is_empty() {
            match tokio::time::timeout_at(deadline, jobs.join_next()).await {
                Ok(Some(Ok(Ok(snapshot)))) => snapshots.push(snapshot),
                Ok(Some(_)) => {}
                _ => break,
            }
        }
        let mut result = aggregate(&snapshots, expected)?;
        result.view = Some(view(settings));
        return Ok(result);
    }
    let account = list
        .iter()
        .find(|a| a["id"].as_str() == Some(settings.antigravity_account.as_str()))
        .ok_or("请在连接设置中选择 Tools 账号")?;
    if !available(account) {
        return Err("Tools 账号已停用或受限，请在 Tools 中检查".into());
    }
    let mut snapshot = query_account(&connection, account, settings, force).await?;
    snapshot.details.push(Detail {
        label: "账号".into(),
        value: label(account),
    });
    Ok(snapshot)
}

async fn query_account(
    connection: &Connection,
    account: &Value,
    settings: &Settings,
    force: bool,
) -> Result<Snapshot, String> {
    let now = Utc::now().timestamp();
    let cached = parse(&account["quota"], settings, now);
    if !force {
        if let Ok(ref s) = cached {
            if s.status == "ready"
                && s.updated_at
                    .is_some_and(|at| now - at < settings.antigravity_minutes as i64 * 60)
            {
                return cached;
            }
        }
    }
    match connection
        .get(&format!("/accounts/{}/quota", settings.antigravity_account))
        .await
    {
        Ok(quota) => parse(&quota, settings, now),
        Err(error) => match cached {
            Ok(mut snapshot) => {
                snapshot.status = "stale".into();
                snapshot.message = error;
                Ok(snapshot)
            }
            Err(_) => Err(error),
        },
    }
}

fn aggregate(snapshots: &[Snapshot], expected: usize) -> Result<Snapshot, String> {
    use std::collections::BTreeMap;
    if snapshots.is_empty() {
        return Err("未能读取综合额度，请检查服务和账号状态".into());
    }
    let mut groups: BTreeMap<String, Vec<&Metric>> = BTreeMap::new();
    for snapshot in snapshots {
        let mut seen = HashSet::new();
        for metric in &snapshot.metrics {
            if metric.unit == "%"
                && metric.remaining_percent.is_some()
                && seen.insert(metric.label.clone())
            {
                groups.entry(metric.label.clone()).or_default().push(metric);
            }
        }
    }
    if groups.is_empty() {
        return Err("没有可用于综合视图的额度数据".into());
    }
    let incomplete = snapshots.len() != expected
        || snapshots.iter().any(|s| s.status != "ready")
        || groups.values().any(|g| g.len() != expected);
    let mut result = Snapshot::empty(
        "antigravity",
        if incomplete { "stale" } else { "ready" },
        if incomplete {
            "综合平均：部分账号数据缺失或过期，请展开详情查看"
        } else {
            "综合平均 · 各账号等权"
        },
    );
    result.updated_at = snapshots.iter().filter_map(|s| s.updated_at).min();
    for (label, metrics) in groups {
        let average = metrics
            .iter()
            .filter_map(|m| m.remaining_percent)
            .sum::<f64>()
            / metrics.len() as f64;
        // Accounts reset independently; a combined allowance has no single reset time.
        result.metrics.push(percent_metric(label, average, None));
    }
    result.details.push(Detail { label: "汇总口径".into(), value: "各账号等权平均，非实际容量加权；五小时和每周、不同额度池分别统计，耗尽账号的 0% 参与平均".into() });
    result.details.push(Detail {
        label: "数据完整度".into(),
        value: format!(
            "取得 {} / {} 个参与账号的数据；缺失值不按零处理，停用或受限账号不参与",
            snapshots.len(),
            expected
        ),
    });
    Ok(result)
}

fn reset(value: &Value) -> Option<i64> {
    value
        .as_str()
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|t| t.timestamp())
}
fn percent_metric(label: String, percent: f64, at: Option<i64>) -> Metric {
    Metric {
        label,
        value: format!("{:.1}", percent)
            .trim_end_matches('0')
            .trim_end_matches('.')
            .into(),
        unit: "%".into(),
        remaining_percent: Some(percent),
        resets_at: at,
    }
}
pub fn parse(quota: &Value, settings: &Settings, now: i64) -> Result<Snapshot, String> {
    if quota["is_forbidden"].as_bool() == Some(true) {
        return Err("Tools 账号额度访问受限，请在 Tools 中检查".into());
    }
    let mut snapshot = Snapshot::empty("antigravity", "ready", "来自 Antigravity Tools");
    snapshot.view = Some(view(settings));
    snapshot.updated_at = quota["last_updated"]
        .as_i64()
        .filter(|t| *t > 0 && *t <= now + 60);
    let mut seen = HashSet::new();
    let mut incomplete = false;
    let mut selected_groups = 0;
    if let Some(groups) = quota["quota_groups"]
        .as_array()
        .filter(|groups| !groups.is_empty())
    {
        if settings.antigravity_groups.iter().any(|name| {
            !groups.iter().any(|g| {
                g["display_name"]
                    .as_str()
                    .is_some_and(|actual| matches_selection(name, actual))
            })
        }) {
            incomplete = true;
        }
        for group in groups {
            let name = group["display_name"].as_str().unwrap_or("共享额度");
            if !settings.antigravity_groups.is_empty()
                && !settings
                    .antigravity_groups
                    .iter()
                    .any(|g| matches_selection(g, name))
            {
                continue;
            }
            selected_groups += 1;
            if group["buckets"].as_array().is_none_or(|b| b.is_empty()) {
                incomplete = true;
            }
            for bucket in group["buckets"].as_array().into_iter().flatten() {
                let Some(id) = bucket["bucket_id"].as_str().filter(|id| !id.is_empty()) else {
                    incomplete = true;
                    continue;
                };
                if !seen.insert(id.to_string()) {
                    continue;
                }
                let Some(fraction) = bucket["remaining_fraction"]
                    .as_f64()
                    .filter(|f| (0.0..=1.0).contains(f))
                else {
                    incomplete = true;
                    continue;
                };
                let window = bucket["window"].as_str().unwrap_or(id);
                let caption = match window {
                    "5h" => "5 小时",
                    "weekly" => "每周",
                    other => other,
                };
                let at = reset(&bucket["reset_time"]);
                if let Some(observed) = bucket["observed_at"]
                    .as_i64()
                    .map(|t| t / 1000)
                    .filter(|t| *t > 0 && *t <= now + 60)
                {
                    snapshot.updated_at =
                        Some(snapshot.updated_at.map_or(observed, |at| at.min(observed)));
                }
                snapshot.metrics.push(percent_metric(
                    format!("{name} · {caption}"),
                    fraction * 100.,
                    at,
                ));
            }
        }
    } else {
        for model in quota["models"].as_array().into_iter().flatten() {
            let Some(name) = model["name"].as_str() else {
                continue;
            };
            if !settings.antigravity_groups.is_empty()
                && !settings
                    .antigravity_groups
                    .iter()
                    .any(|g| matches_selection(g, name))
            {
                continue;
            }
            if !seen.insert(name.to_string()) {
                continue;
            }
            let Some(percent) = model["percentage"]
                .as_f64()
                .filter(|p| (0.0..=100.).contains(p))
            else {
                incomplete = true;
                continue;
            };
            snapshot.metrics.push(percent_metric(
                name.to_string(),
                percent,
                reset(&model["reset_time"]),
            ));
        }
        snapshot.details.push(Detail {
            label: "额度结构".into(),
            value: "Tools 仅返回模型额度，未提供双窗口共享池；各模型不可相加".into(),
        });
    }
    if snapshot.metrics.is_empty() {
        return Err(
            if selected_groups == 0 && !settings.antigravity_groups.is_empty() {
                "选中的额度池已变化，请重新选择"
            } else {
                "Tools 未返回有效额度，暂时无法统计"
            }
            .into(),
        );
    }
    snapshot.details.push(Detail {
        label: "数据来源".into(),
        value: "Antigravity Tools；按实际额度池展示，共享额度不累加".into(),
    });
    snapshot.details.push(Detail {
        label: "更新时间口径".into(),
        value: "沿用 Tools 来源时间；缺少分项观测时间时无法确认每个窗口的新鲜度".into(),
    });
    if incomplete
        || snapshot
            .updated_at
            .is_none_or(|at| now - at > settings.antigravity_minutes as i64 * 120)
        || snapshot
            .metrics
            .iter()
            .any(|m| m.resets_at.is_some_and(|at| at <= now))
    {
        snapshot.status = "stale".into();
        snapshot.message = "Tools 数据待确认：部分额度缺失、来源较旧或已到重置时间".into();
    }
    Ok(snapshot)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn quota() -> Value {
        json!({"last_updated":1800000000,"quota_groups":[{"display_name":"Gemini","buckets":[{"bucket_id":"g5","window":"5h","remaining_fraction":0.8,"reset_time":"2027-01-20T12:00:00Z"},{"bucket_id":"gw","window":"weekly","remaining_fraction":0.24,"reset_time":"2027-01-21T12:00:00Z"}]}]})
    }
    #[test]
    fn switching_views_preserves_values_time_and_original_identity() {
        let old = Settings {
            antigravity_account: "account-a".into(),
            antigravity_groups: vec!["Gemini Models".into()],
            ..Settings::default()
        };
        let snapshot = parse(&quota(), &old, 1800000010).unwrap();
        let retained = retain_view(Some(&snapshot), &old);
        assert_eq!(retained.status, "stale");
        assert_eq!(retained.updated_at, snapshot.updated_at);
        assert_eq!(
            serde_json::to_value(&retained.metrics).unwrap(),
            serde_json::to_value(&snapshot.metrics).unwrap()
        );
        assert_eq!(retained.view.unwrap().account, "account-a");
        let empty = retain_view(None, &old);
        assert!(empty.metrics.is_empty());
        assert_eq!(empty.status, "idle");
    }
    #[test]
    fn global_group_selection_applies_to_every_account_and_combined_average() {
        let mut q = quota();
        let mut claude = q["quota_groups"][0].clone();
        claude["display_name"] = json!("Claude & GPT Models");
        claude["buckets"][0]["bucket_id"] = json!("c5");
        claude["buckets"][1]["bucket_id"] = json!("cw");
        q["quota_groups"].as_array_mut().unwrap().push(claude);
        let mut snapshots = vec![];
        for id in ["account-a", "account-b"] {
            let settings = Settings {
                antigravity_account: id.into(),
                antigravity_groups: vec!["Claude & GPT Models".into()],
                ..Settings::default()
            };
            let snapshot = parse(&q, &settings, 1800000010).unwrap();
            assert_eq!(snapshot.metrics.len(), 2);
            assert!(snapshot
                .metrics
                .iter()
                .all(|m| m.label.starts_with("Claude & GPT Models")));
            snapshots.push(snapshot);
        }
        let combined = aggregate(&snapshots, 2).unwrap();
        assert_eq!(combined.metrics.len(), 2);
        assert!(combined
            .metrics
            .iter()
            .all(|m| m.label.starts_with("Claude & GPT Models")));
        let gemini = Settings {
            antigravity_groups: vec!["Gemini Models".into()],
            ..Settings::default()
        };
        assert!(parse(&q, &gemini, 1800000010)
            .unwrap()
            .metrics
            .iter()
            .all(|m| m.label.starts_with("Gemini")));
        assert!(matches_selection("Claude & GPT Models", "Claude"));
    }
    #[test]
    fn accepts_loopback_and_secure_cloud_roots_with_proxy_prefixes() {
        assert_eq!(
            normalize_address(" localhost:8045 ").unwrap(),
            "http://localhost:8045"
        );
        assert_eq!(
            normalize_address("http://127.0.0.1:9000/api/").unwrap(),
            "http://127.0.0.1:9000"
        );
        assert_eq!(
            normalize_address("http://[::1]:8045").unwrap(),
            "http://[::1]:8045"
        );
        assert_eq!(
            normalize_address("https://tools.example.com/prefix/api/").unwrap(),
            "https://tools.example.com/prefix"
        );
        let legacy = Settings {
            antigravity_port: 9011,
            ..Settings::default()
        };
        assert_eq!(service_base(&legacy).unwrap(), "http://127.0.0.1:9011");
    }
    #[test]
    fn rejects_insecure_cloud_inference_urls_and_embedded_credentials() {
        for address in [
            "",
            " ",
            "http://example.com:8045",
            "http://192.168.1.2:8045",
            "ftp://example.com",
            "https://user:pass@example.com",
            "https://example.com?key=fixture",
            "https://example.com/#fragment",
            "https://example.com/v1",
            "https://example.com/v1/chat/completions",
            "http://localhost:0",
            "http://localhost\\example.com",
            "https://exa\nmple.com",
        ] {
            assert!(
                normalize_address(address).is_err(),
                "must reject {address:?}"
            );
        }
    }
    #[test]
    fn credentials_are_bound_to_the_exact_normalized_service() {
        let local = Settings::default();
        let encoded = encode_key(&local, "fixture-key").unwrap();
        assert_eq!(decode_key(&local, &encoded).unwrap(), "fixture-key");
        let other = Settings {
            antigravity_url: "https://tools.example.com".into(),
            ..local.clone()
        };
        let error = decode_key(&other, &encoded).unwrap_err();
        assert!(!error.contains("fixture-key"));
        assert!(decode_key(&local, "legacy-key").is_err());
    }
    #[test]
    fn aggregate_counts_zero_separates_windows_and_has_no_invented_reset() {
        let a = parse(&quota(), &Settings::default(), 1800000010).unwrap();
        let mut b = a.clone();
        b.metrics[0].remaining_percent = Some(0.);
        b.metrics[0].value = "0".into();
        let result = aggregate(&[a, b], 2).unwrap();
        assert_eq!(result.metrics.len(), 2);
        assert_eq!(result.metrics[0].remaining_percent, Some(40.));
        assert_eq!(result.metrics[1].remaining_percent, Some(24.));
        assert!(result.metrics.iter().all(|m| m.resets_at.is_none()));
        assert_eq!(result.updated_at, Some(1800000000));
    }
    #[test]
    fn incomplete_aggregate_is_marked_and_unknown_accounts_are_not_zero() {
        let mut snapshot = Snapshot::success("antigravity");
        snapshot
            .metrics
            .push(percent_metric("Claude · 5 小时".into(), 80., None));
        let result = aggregate(&[snapshot], 2).unwrap();
        assert_eq!(result.status, "stale");
        assert_eq!(result.metrics[0].remaining_percent, Some(80.));
        assert!(aggregate(&[], 2).is_err());
    }
    #[tokio::test]
    async fn redirects_never_forward_a_management_key_to_another_server() {
        use std::io::{Read, Write};
        let source = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let target = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        target.set_nonblocking(true).unwrap();
        let base = format!("http://{}/api", source.local_addr().unwrap());
        let location = format!("http://{}/api/accounts", target.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let (mut stream, _) = source.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut buffer = [0; 4096];
            let n = stream.read(&mut buffer).unwrap();
            let request = String::from_utf8_lossy(&buffer[..n]);
            assert!(request
                .to_lowercase()
                .contains("authorization: bearer fixture-key"));
            stream.write_all(format!("HTTP/1.1 302 Found\r\nLocation: {location}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").as_bytes()).unwrap();
        });
        let connection = Connection {
            client: reqwest::Client::builder()
                .no_proxy()
                .redirect(reqwest::redirect::Policy::none())
                .timeout(Duration::from_secs(3))
                .build()
                .unwrap(),
            base,
            key: "fixture-key".into(),
        };
        assert!(connection.get("/accounts").await.is_err());
        server.join().unwrap();
        assert_eq!(
            target.accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
    }
    #[test]
    fn preserves_both_windows_and_deduplicates_shared_buckets() {
        let mut q = quota();
        let duplicate = q["quota_groups"][0].clone();
        q["quota_groups"].as_array_mut().unwrap().push(duplicate);
        let s = parse(&q, &Settings::default(), 1800000010).unwrap();
        assert_eq!(s.metrics.len(), 2);
        assert_eq!(s.metrics[0].remaining_percent, Some(80.));
        assert_eq!(s.metrics[1].remaining_percent, Some(24.));
        assert!(s.metrics[0].resets_at.unwrap() > 1800000010);
        assert_eq!(s.updated_at, Some(1800000000));
    }
    #[test]
    fn missing_quota_is_unknown_and_bad_fields_are_stale() {
        assert!(parse(&json!({}), &Settings::default(), 1800000010).is_err());
        let mut q = quota();
        q["quota_groups"][0]["buckets"][0]["remaining_fraction"] = Value::Null;
        let s = parse(&q, &Settings::default(), 1800000010).unwrap();
        assert_eq!(s.status, "stale");
        assert_eq!(s.metrics.len(), 1);
        assert_eq!(s.metrics[0].remaining_percent, Some(24.));
    }
    #[test]
    fn old_observations_and_reached_resets_are_not_refilled() {
        let mut q = quota();
        q["quota_groups"][0]["buckets"][0]["observed_at"] = json!(1799900000000_i64);
        let s = parse(&q, &Settings::default(), 1800000010).unwrap();
        assert_eq!(s.status, "stale");
        assert_eq!(s.updated_at, Some(1799900000));
        q["quota_groups"][0]["buckets"][0]["reset_time"] = json!("2020-01-01T00:00:00Z");
        assert_eq!(
            parse(&q, &Settings::default(), 1800000010).unwrap().metrics[0].remaining_percent,
            Some(80.)
        );
    }
    #[test]
    fn account_path_cannot_escape_and_group_selection_is_respected() {
        let mut s = Settings::default();
        s.antigravity_account = "../config".into();
        assert!(validate(&s).is_err());
        s.antigravity_account = "test-id".into();
        s.antigravity_groups = vec!["Claude".into()];
        assert!(parse(&quota(), &s, 1800000010).is_err());
        s.antigravity_groups.push("Gemini".into());
        assert_eq!(parse(&quota(), &s, 1800000010).unwrap().status, "stale");
    }
}
