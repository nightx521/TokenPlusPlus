//! Read-only Agent2API management client. Only whitelisted display fields leave this module.
#[cfg(test)]
#[path = "agent2api_tests.rs"]
mod network_tests;
use crate::{
    antigravity,
    model::{Metric, Settings, Snapshot},
    providers,
};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{HashMap, HashSet},
    str::FromStr,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Selection {
    pub version: u32,
    pub accounts: Option<Vec<String>>,
    pub models: HashMap<String, Vec<String>>,
    pub metrics: HashMap<String, Vec<String>>,
}
#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Model {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub target: String,
    pub brand: String,
    pub enabled: bool,
}
#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Reading {
    pub id: String,
    #[serde(flatten)]
    pub metric: Metric,
    pub model_ids: Vec<String>,
    pub shared: bool,
}
#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub enabled: bool,
    pub models: Vec<Model>,
    pub metrics: Vec<Reading>,
    pub status: String,
    pub message: String,
    pub updated_at: Option<i64>,
}
#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Data {
    pub version: u32,
    pub service: String,
    pub accounts: Vec<Account>,
}

pub fn base(settings: &Settings) -> Result<String, String> {
    antigravity::normalize_address(&settings.agent2api_url).map_err(|_| {
        "请填写有效管理服务根地址：云端 HTTPS，本机可用 localhost:端口；不要带 /v1".into()
    })
}
pub fn validate(settings: &Settings) -> Result<(), String> {
    if !settings.agent2api_url.is_empty() {
        base(settings)?;
    }
    if !(1..=120).contains(&settings.agent2api_minutes) || settings.agent2api_selection.version > 1
    {
        return Err("Agent2API 刷新间隔或显示配置无效".into());
    }
    let selection = &settings.agent2api_selection;
    if selection.accounts.as_ref().is_some_and(|a| a.len() > 2000)
        || selection.models.len() > 2000
        || selection.metrics.len() > 2000
        || selection
            .models
            .values()
            .chain(selection.metrics.values())
            .any(|v| v.len() > 5000 || v.iter().any(|s| s.len() > 1024))
    {
        return Err("Agent2API 选择项目过多".into());
    }
    Ok(())
}
static BLOCKED: std::sync::OnceLock<std::sync::Mutex<HashSet<String>>> = std::sync::OnceLock::new();
fn blocked() -> &'static std::sync::Mutex<HashSet<String>> {
    BLOCKED.get_or_init(Default::default)
}
#[derive(Serialize, Deserialize)]
struct Bound {
    base: String,
    key: String,
}
fn decode(settings: &Settings, value: &str) -> Result<String, String> {
    let b: Bound = serde_json::from_str(value).map_err(|_| "请重新保存管理令牌")?;
    if b.base != base(settings)? || b.key.is_empty() {
        return Err("服务地址已变化，请为此地址重新填写管理令牌".into());
    }
    Ok(b.key)
}
pub fn credential(settings: &Settings) -> Result<String, String> {
    decode(settings, &providers::credential("agent2api")?)
}
pub fn save_credential(settings: &Settings, key: &str) -> Result<(), String> {
    if key.trim().is_empty() || key.len() > 16000 || key.chars().any(char::is_control) {
        return Err("管理令牌格式无效".into());
    }
    let value = serde_json::to_string(&Bound {
        base: base(settings)?,
        key: key.trim().into(),
    })
    .map_err(|_| "凭据编码失败")?;
    let result = keyring::Entry::new("TokenPlusPlus", "agent2api")
        .map_err(|_| "无法访问系统凭据存储")?
        .set_password(&value)
        .map_err(|_| String::from("管理令牌保存失败"));
    if result.is_ok() {
        blocked().lock().unwrap().remove(&base(settings)?);
    }
    result
}
#[derive(Clone)]
struct Connection {
    client: reqwest::Client,
    base: String,
    key: String,
    blocked: Arc<AtomicBool>,
}
impl Connection {
    fn new(settings: &Settings) -> Result<Self, String> {
        validate(settings)?;
        if blocked().lock().unwrap().contains(&base(settings)?) {
            return Err("管理令牌失效，请在设置中更新".into());
        }
        Ok(Self {
            client: reqwest::Client::builder()
                .no_proxy()
                .redirect(reqwest::redirect::Policy::none())
                .timeout(Duration::from_secs(30))
                .build()
                .map_err(|_| "无法建立连接")?,
            base: base(settings)?,
            key: credential(settings)?,
            blocked: Arc::new(AtomicBool::new(false)),
        })
    }
    async fn get(&self, path: &str, id: Option<&str>) -> Result<Value, String> {
        if self.blocked.load(Ordering::Relaxed) {
            return Err("管理令牌失效，请在设置中更新".into());
        }
        let mut request = self
            .client
            .get(format!("{}{path}", self.base))
            .bearer_auth(&self.key);
        if let Some(id) = id {
            request = request.query(&[("id", id)]);
        }
        let mut response = request
            .send()
            .await
            .map_err(|_| "无法连接 Agent2API，请检查网络、地址和证书")?;
        let code = response.status().as_u16();
        if code == 401 {
            self.blocked.store(true, Ordering::Relaxed);
            blocked().lock().unwrap().insert(self.base.clone());
            return Err("管理令牌失效，请在设置中更新".into());
        }
        if code == 429 {
            providers::record_retry_after(
                "agent2api",
                response
                    .headers()
                    .get("Retry-After")
                    .and_then(|v| v.to_str().ok()),
            );
            return Err("Agent2API 限流，请稍后重试".into());
        }
        if code != 200 {
            return Err(match code {
                403 => "管理接口拒绝访问，请检查令牌权限".into(),
                404 => "此版本不提供所需管理接口，请检查版本及服务根地址".into(),
                300..=399 => "管理地址发生重定向，已停止发送令牌，请填写最终地址".into(),
                _ => format!("Agent2API 查询失败（HTTP {code}）"),
            });
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| "读取响应失败")? {
            if bytes.len() + chunk.len() > 8 * 1024 * 1024 {
                return Err("管理响应过大，已停止读取".into());
            }
            bytes.extend_from_slice(&chunk);
        }
        let v: Value = serde_json::from_slice(&bytes).map_err(|_| "管理接口返回格式不兼容")?;
        if v.get("success") == Some(&Value::Bool(false)) {
            return Err("管理接口查询失败，请在 Agent2API 检查状态".into());
        }
        Ok(v.get("data").cloned().unwrap_or(v))
    }
}
fn text(v: &Value, key: &str) -> String {
    v.get(key)
        .and_then(Value::as_str)
        .unwrap_or("")
        .chars()
        .filter(|c| !c.is_control())
        .take(256)
        .collect()
}
fn rows<'a>(v: &'a Value, key: &str) -> &'a [Value] {
    v.get(key)
        .or_else(|| v.as_array().map(|_| v))
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
}
fn decimal(v: &Value, keys: &[&str]) -> Option<Decimal> {
    keys.iter().find_map(|k| {
        let n = v.get(k)?;
        if n.is_null() || n.is_boolean() {
            return None;
        }
        Decimal::from_str(
            &n.as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| n.to_string()),
        )
        .ok()
    })
}
fn timestamp(v: &Value, keys: &[&str]) -> Option<i64> {
    keys.iter().find_map(|k| {
        let x = v.get(k)?;
        let n = x.as_i64().or_else(|| x.as_str()?.parse().ok());
        if let Some(n) = n.filter(|n| *n > 0) {
            return Some(if n > 100_000_000_000 { n / 1000 } else { n });
        }
        chrono::DateTime::parse_from_rfc3339(x.as_str()?)
            .ok()
            .map(|t| t.timestamp())
    })
}

pub fn readings(usage: &Value) -> Vec<Reading> {
    let mut out = Vec::new();
    let default_unit = text(usage, "unit");
    let unit = if default_unit.is_empty() {
        String::new()
    } else {
        default_unit
    };
    let mut add = |id: String,
                   label: String,
                   v: &Value,
                   unit: String,
                   remaining: Option<Decimal>,
                   total: Option<Decimal>| {
        let Some(value) = remaining else { return };
        let percent = if id == "used" || id == "total" {
            None
        } else if unit == "%" {
            value
                .to_string()
                .parse::<f64>()
                .ok()
                .filter(|n| (0.0..=100.0).contains(n))
        } else {
            total
                .filter(|t| *t > Decimal::ZERO && value >= Decimal::ZERO && value <= *t)
                .and_then(|t| ((value / t) * Decimal::from(100)).to_string().parse().ok())
        };
        let model = text(v, "modelId");
        let model_ids = if model.is_empty() {
            vec![]
        } else {
            vec![model]
        };
        out.push(Reading {
            id,
            metric: Metric {
                label,
                value: value.normalize().to_string(),
                unit,
                remaining_percent: percent,
                resets_at: timestamp(v, &["resetsAt", "resetAt", "refreshAt"]),
            },
            shared: model_ids.is_empty(),
            model_ids,
        });
    };
    if usage.get("unlimited") == Some(&Value::Bool(true)) {
        return vec![];
    }
    // Document-level available already includes wallets; do not sum it with wallet rows.
    add(
        "available".into(),
        "可用额度（共享）".into(),
        usage,
        unit.clone(),
        decimal(
            usage,
            &[
                "available",
                "usageLeft",
                "totalLeft",
                "remaining",
                "balance",
            ],
        ),
        decimal(usage, &["usageTotal", "total", "limit"]),
    );
    if let Some(used) = decimal(usage, &["usageUsed", "used"]) {
        add(
            "used".into(),
            "已用额度".into(),
            usage,
            unit.clone(),
            Some(used),
            None,
        );
    }
    if let Some(total) = decimal(usage, &["usageTotal", "total", "limit"]) {
        add(
            "total".into(),
            "总额度".into(),
            usage,
            unit.clone(),
            Some(total),
            None,
        );
    }
    for (key, label) in [("planLeft", "套餐剩余额度"), ("bonusLeft", "赠送剩余额度")] {
        add(
            key.into(),
            label.into(),
            usage,
            unit.clone(),
            decimal(usage, &[key]),
            None,
        );
    }
    for (index, w) in rows(usage, "credits").iter().enumerate() {
        let now = chrono::Utc::now().timestamp();
        if timestamp(w, &["expireAt"]).is_some_and(|at| at <= now)
            || timestamp(w, &["startAt"]).is_some_and(|at| at > now)
        {
            continue;
        }
        let id = text(w, "id");
        let name = text(w, "name");
        let granted = timestamp(w, &["grantAt"]);
        add(
            format!(
                "credit:{}",
                if id.is_empty() {
                    format!(
                        "{name}:{}",
                        granted
                            .map(|n| n.to_string())
                            .unwrap_or_else(|| index.to_string())
                    )
                } else {
                    id
                }
            ),
            if name.is_empty() {
                format!("额度明细 {}", index + 1)
            } else {
                name
            },
            w,
            unit.clone(),
            decimal(w, &["remain"]),
            decimal(w, &["total"]),
        );
    }
    for (index, w) in rows(usage, "wallets").iter().enumerate() {
        let label = text(w, "displayName");
        let kind = text(w, "type");
        let u = text(w, "unit");
        let pool = text(w, "poolId");
        let id = text(w, "id");
        let identity = if !pool.is_empty() {
            pool
        } else if !id.is_empty() {
            id
        } else {
            format!("{kind}:{}:{}", text(w, "displayName"), text(w, "unit"))
        };
        add(
            format!("wallet:{identity}"),
            if label.is_empty() {
                format!("明细 {}", index + 1)
            } else {
                label
            },
            w,
            if u.is_empty() { unit.clone() } else { u },
            decimal(w, &["balance", "remaining"]),
            None,
        );
        // Explicit token fields form a separate Token pool, never a currency/credits conversion.
        if let (Some(total), Some(used)) = (
            decimal(w, &["allowanceTokens"]),
            decimal(w, &["usedTokens"]),
        ) {
            if used >= Decimal::ZERO && total >= used {
                add(
                    format!("wallet-token:{kind}:{index}"),
                    format!("{} · Token", text(w, "displayName")),
                    w,
                    "Token".into(),
                    Some(total - used),
                    Some(total),
                );
            }
        }
    }
    for (index, w) in rows(usage, "windows").iter().enumerate() {
        let key = text(w, "key");
        let label = text(w, "label");
        add(
            format!(
                "window:{}",
                if key.is_empty() {
                    index.to_string()
                } else {
                    key.clone()
                }
            ),
            if label.is_empty() { key } else { label },
            w,
            "%".into(),
            decimal(w, &["remainingPercent"]),
            Some(Decimal::from(100)),
        );
    }
    let mut seen = HashSet::new();
    out.retain(|r| seen.insert(r.id.clone()));
    out
}
fn catalog(accounts: &Value, models: &Value) -> Result<Vec<Account>, String> {
    if !accounts.get("accounts").is_some_and(Value::is_array)
        || !models.get("models").is_some_and(Value::is_array)
    {
        return Err("账号或模型目录格式不兼容，请核对 Agent2API 版本".into());
    }
    let mut all: Vec<Model> = rows(models, "models")
        .iter()
        .filter_map(|v| {
            let id = text(v, "id");
            if id.is_empty() {
                return None;
            }
            Some(Model {
                id: id.clone(),
                name: {
                    let name = text(v, "name");
                    if name.is_empty() {
                        id.clone()
                    } else {
                        name
                    }
                },
                brand: text(v, "brand"),
                provider: text(v, "provider"),
                target: id,
                enabled: v.get("enabled") != Some(&Value::Bool(false)),
            })
        })
        .collect();
    for v in rows(models, "mappings") {
        let id = text(v, "alias");
        let target = text(v, "target");
        if !id.is_empty() && !target.is_empty() {
            all.push(Model {
                id: id.clone(),
                name: id,
                brand: text(v, "brand"),
                provider: text(v, "provider"),
                target,
                enabled: v.get("enabled") != Some(&Value::Bool(false)),
            });
        }
    }
    let mut seen = HashSet::new();
    let result = rows(accounts, "accounts")
        .iter()
        .filter_map(|v| {
            let id = text(v, "id");
            if id.is_empty() || !seen.insert(id.clone()) {
                return None;
            }
            let provider = text(v, "provider");
            let mut unique = HashSet::new();
            Some(Account {
                id: id.clone(),
                name: {
                    let n = text(v, "name");
                    if n.is_empty() {
                        id
                    } else {
                        n
                    }
                },
                models: all
                    .iter()
                    .rev()
                    .filter(|m| !provider.is_empty() && m.provider == provider)
                    .filter(|m| unique.insert(m.id.clone()))
                    .cloned()
                    .collect(),
                provider,
                enabled: v.get("enabled") != Some(&Value::Bool(false))
                    && v.get("available") != Some(&Value::Bool(false)),
                metrics: vec![],
                status: "idle".into(),
                message: "等待查询".into(),
                updated_at: None,
            })
        })
        .collect();
    Ok(result)
}
pub fn selected(settings: &Settings, a: &Account) -> bool {
    settings
        .agent2api_selection
        .accounts
        .as_ref()
        .map_or(a.enabled, |ids| ids.contains(&a.id))
}
pub fn retain(mut new: Snapshot, old: Option<&Snapshot>) -> Snapshot {
    if let (Some(data), Some(previous)) =
        (new.gateway.as_mut(), old.and_then(|s| s.gateway.as_ref()))
    {
        if data.service == previous.service {
            for a in &mut data.accounts {
                if a.status != "ready" && a.metrics.is_empty() {
                    if let Some(p) = previous.accounts.iter().find(|p| p.id == a.id) {
                        a.metrics = p.metrics.clone();
                        a.updated_at = p.updated_at;
                        if !a.metrics.is_empty() {
                            a.status = "stale".into();
                        }
                    }
                }
            }
        }
    }
    new
}
pub async fn query(settings: &Settings, _force: bool) -> Result<Snapshot, String> {
    let c = Connection::new(settings)?;
    query_connection(settings, c).await
}
async fn query_connection(settings: &Settings, c: Connection) -> Result<Snapshot, String> {
    let accounts = c.get("/api/accounts", None).await?;
    let models = c.get("/api/models/manage", None).await?;
    let mut data = Data {
        version: 1,
        service: c.base.clone(),
        accounts: catalog(&accounts, &models)?,
    };
    let snapshot = c.get("/api/accounts/usage/snapshot", None).await?;
    let now = chrono::Utc::now().timestamp();
    for a in &mut data.accounts {
        if let Some(row) = rows(&snapshot, "results")
            .iter()
            .find(|r| text(r, "id") == a.id)
        {
            apply_row(a, row, timestamp(&snapshot, &["at"]), now);
        }
    }
    let ids: Vec<usize> = data
        .accounts
        .iter()
        .enumerate()
        .filter(|(_, a)| selected(settings, a))
        .map(|(i, _)| i)
        .collect();
    for batch in ids.chunks(4) {
        let mut tasks = tokio::task::JoinSet::new();
        for &i in batch {
            let c = c.clone();
            let id = data.accounts[i].id.clone();
            tasks.spawn(async move { (i, c.get("/api/accounts/usage", Some(&id)).await) });
        }
        while let Some(task) = tasks.join_next().await {
            let (i, result) = task.map_err(|_| "额度查询进程退出")?;
            let a = &mut data.accounts[i];
            match result {
                Ok(v) => {
                    if let Some(row) = rows(&v, "results").iter().find(|r| text(r, "id") == a.id) {
                        apply_row(a, row, Some(now), now);
                    } else {
                        a.status = "error".into();
                        a.message = "查询未返回此账号的结果".into();
                    }
                }
                Err(e) => {
                    a.status = if a.metrics.is_empty() {
                        "error"
                    } else {
                        "stale"
                    }
                    .into();
                    a.message = e;
                }
            }
        }
        if c.blocked.load(Ordering::Relaxed) {
            return Err("管理令牌失效，请在设置中更新".into());
        }
    }
    let partial = data
        .accounts
        .iter()
        .any(|a| selected(settings, a) && matches!(a.status.as_str(), "stale" | "error"));
    let mut result = Snapshot::success("agent2api");
    result.status = if partial { "stale" } else { "ready" }.into();
    result.message = if partial {
        "部分账号查询未成功，保留上次数据"
    } else {
        "管理接口 · 按账号显示，共享额度不重复汇总"
    }
    .into();
    result.gateway = Some(data);
    Ok(result)
}
fn apply_row(a: &mut Account, row: &Value, fallback: Option<i64>, now: i64) {
    if row
        .get("error")
        .is_some_and(|v| !v.is_null() && v != &Value::String(String::new()))
    {
        a.status = "error".into();
        a.message = "账号额度查询失败，请在 Agent2API 检查账号状态".into();
        return;
    }
    let usage = &row["usage"];
    if !usage.is_object() {
        a.status = "unsupported".into();
        a.message = "没有可识别的额度数据".into();
        return;
    }
    // ZCode confirms no active packages with an explicit empty plans response.
    // This does not apply to missing fields, failed requests or other providers.
    let no_zcode_plan = a.provider == "zcode"
        && usage.get("available") == Some(&Value::Null)
        && usage
            .get("plans")
            .and_then(Value::as_array)
            .is_some_and(|v| v.is_empty())
        && usage
            .get("wallets")
            .and_then(Value::as_array)
            .is_some_and(|v| v.is_empty());
    if no_zcode_plan {
        let mut known = usage.clone();
        known["available"] = serde_json::json!(0);
        a.metrics = readings(&known);
    } else {
        a.metrics = readings(usage);
    }

    a.updated_at = timestamp(row, &["at"]).or(fallback);
    a.status = if a.metrics.is_empty() {
        "unsupported"
    } else if a
        .updated_at
        .is_none_or(|at| now - at > 600 || at > now + 60)
    {
        "stale"
    } else {
        "ready"
    }
    .into();
    a.message = if usage.get("unlimited") == Some(&Value::Bool(true)) {
        "上游标记为不限量"
    } else if a.metrics.is_empty() {
        "额度未知或当前格式不支持"
    } else if a.status == "stale" {
        "上次数据，等待更新确认"
    } else {
        ""
    }
    .into();
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn explicit_empty_zcode_plan_is_zero_but_missing_or_failed_data_is_not() {
        let mut a = catalog(
            &json!({"accounts":[{"id":"a","provider":"zcode"}]}),
            &json!({"models":[]}),
        )
        .unwrap()
        .remove(0);
        let now = chrono::Utc::now().timestamp();
        apply_row(
            &mut a,
            &json!({"usage":{"available":null,"plans":[],"wallets":[]}}),
            Some(now),
            now,
        );
        assert_eq!(a.status, "ready");
        assert_eq!(a.metrics[0].metric.value, "0");
        assert_eq!(a.metrics[0].metric.unit, "");
        a.metrics.clear();
        apply_row(&mut a, &json!({"usage":{"available":null}}), Some(now), now);
        assert!(a.metrics.is_empty());
        apply_row(
            &mut a,
            &json!({"error":"failed","usage":{"available":null,"plans":[],"wallets":[]}}),
            Some(now),
            now,
        );
        assert_eq!(a.status, "error");
        assert!(a.metrics.is_empty());
    }
    #[test]
    fn explicit_wallet_identity_and_model_mapping_take_precedence() {
        let r = readings(
            &json!({"wallets":[{"poolId":"one","displayName":"同名","balance":1,"unit":"USD"},{"poolId":"two","displayName":"同名","balance":2,"unit":"USD"},{"poolId":"one","displayName":"别名","balance":1,"unit":"USD"}]}),
        );
        assert_eq!(r.len(), 2);
        let a=catalog(&json!({"accounts":[{"id":"a","provider":"qoder"}]}),&json!({"models":[{"id":"public-alias","provider":"qoder"}],"mappings":[{"alias":"public-alias","provider":"qoder","target":"glm-4.6"}]})).unwrap();
        assert_eq!(a[0].models[0].target, "glm-4.6");
    }
    #[test]
    fn deployed_legacy_workbuddy_and_codearts_shapes_keep_units_separate() {
        let r = readings(
            &json!({"totalLeft":123,"planLeft":100,"bonusLeft":23,"credits":[{"name":"示例额度","remain":80,"total":100,"used":20,"refreshAt":1800000000000_i64}]}),
        );
        assert_eq!(r[0].metric.value, "123");
        assert_eq!(r.len(), 4);
        assert_eq!(r[3].metric.remaining_percent, Some(80.));
        assert_eq!(r[3].metric.resets_at, Some(1800000000));
        let r = readings(
            &json!({"available":50,"unit":"积分","wallets":[{"type":"model","displayName":"Token 配额","allowanceTokens":1000,"usedTokens":250}]}),
        );
        assert_eq!(r.len(), 2);
        assert_eq!(r[1].metric.unit, "Token");
        assert_eq!(r[1].metric.value, "750");
        assert_eq!(r[0].metric.unit, "积分");
    }
    #[test]
    fn missing_is_unknown_and_units_are_not_guessed() {
        assert!(readings(&json!({"raw":{"balance":99},"available":null})).is_empty());
        let m = readings(&json!({"available":"0","unit":"USD"}));
        assert_eq!(m[0].metric.value, "0");
        assert!(m[0].metric.remaining_percent.is_none());
    }
    #[test]
    fn precise_units_and_shared_pool() {
        let r = readings(
            &json!({"available":"0.30","unit":"CNY","wallets":[{"type":"credit","balance":"0.10","unit":"USD"}]}),
        );
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].metric.value, "0.3");
        assert!(r[0].shared);
        assert_ne!(r[0].metric.unit, r[1].metric.unit);
    }
    #[test]
    fn windows_percent_dates_and_unlimited() {
        let r = readings(
            &json!({"windows":[{"key":"weekly","remainingPercent":27,"resetsAt":1800000000000_i64}]}),
        );
        assert_eq!(r[0].metric.resets_at, Some(1800000000));
        assert_eq!(r[0].metric.remaining_percent, Some(27.));
        assert!(readings(&json!({"unlimited":true,"available":-1})).is_empty());
    }
    #[test]
    fn binding_and_addresses() {
        let s = Settings {
            agent2api_url: "https://example.com/prefix".into(),
            ..Default::default()
        };
        assert!(decode(
            &s,
            r#"{"base":"https://other.example.com","key":"fixture"}"#
        )
        .is_err());
        for url in [
            "http://example.com",
            "https://example.com/v1",
            "https://user:pass@example.com",
        ] {
            assert!(base(&Settings {
                agent2api_url: url.into(),
                ..Default::default()
            })
            .is_err());
        }
    }
    #[test]
    fn catalog_sanitizes_and_maps_alias() {
        let a=catalog(&json!({"accounts":[{"id":"one","name":"示例","provider":"qoder","accessToken":"fixture-must-not-reach-ui"}]}),&json!({"models":[{"id":"claude-sonnet","provider":"qoder"}],"mappings":[{"alias":"my-alias","target":"claude-sonnet","provider":"qoder"}]})).unwrap();
        assert_eq!(a[0].models.len(), 2);
        assert_eq!(a[0].models[1].target, "claude-sonnet");
        assert!(!serde_json::to_string(&a)
            .unwrap()
            .contains("fixture-must-not-reach-ui"));
    }
    #[test]
    fn failure_keeps_timestamp_and_cache_only_same_service() {
        let mut previous = Snapshot::success("agent2api");
        let mut a = catalog(&json!({"accounts":[{"id":"one"}]}), &json!({"models":[]}))
            .unwrap()
            .remove(0);
        a.metrics = readings(&json!({"available":42,"unit":"积分"}));
        a.updated_at = Some(1800000000);
        previous.gateway = Some(Data {
            version: 1,
            service: "https://example.com".into(),
            accounts: vec![a.clone()],
        });
        a.metrics.clear();
        a.updated_at = None;
        a.status = "error".into();
        let mut next = previous.clone();
        next.gateway.as_mut().unwrap().accounts = vec![a];
        let kept = retain(next, Some(&previous));
        assert_eq!(
            kept.gateway.unwrap().accounts[0].updated_at,
            Some(1800000000)
        );
    }
}
