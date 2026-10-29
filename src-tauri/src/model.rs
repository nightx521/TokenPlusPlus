use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Metric {
    pub label: String,
    pub value: String,
    pub unit: String,
    pub remaining_percent: Option<f64>,
    pub resets_at: Option<i64>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Detail {
    pub label: String,
    pub value: String,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct QuotaView {
    pub account: String,
    pub groups: Vec<String>,
    pub service: String,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub provider: String,
    pub status: String,
    pub message: String,
    pub updated_at: Option<i64>,
    pub metrics: Vec<Metric>,
    pub details: Vec<Detail>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub view: Option<QuotaView>,
}
impl Snapshot {
    pub fn empty(provider: &str, status: &str, message: &str) -> Self {
        Self {
            provider: provider.into(),
            status: status.into(),
            message: message.into(),
            updated_at: None,
            metrics: vec![],
            details: vec![],
            view: None,
        }
    }
    pub fn success(provider: &str) -> Self {
        let mut s = Self::empty(provider, "ready", "已连接");
        s.updated_at = Some(chrono::Utc::now().timestamp());
        s
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub always_on_top: bool,
    pub balance_minutes: u64,
    pub quota_minutes: u64,
    pub huawei_site: String,
    pub codex_path: String,
    pub antigravity_port: u16,
    pub antigravity_url: String,
    pub antigravity_account: String,
    pub antigravity_groups: Vec<String>,
    pub antigravity_minutes: u64,
    pub desktop_mode: bool,
    pub compact: bool,
    pub live_background: bool,
    pub glass: String,
    pub glass_tone: String,
    pub opacity: u32,
    pub font_scale: u32,
    pub position_locked: bool,
    pub show_expiry: bool,
    pub visible_platforms: Vec<String>,
    pub desktop_position: Option<(i32, i32)>,
    pub panel_bounds: Option<(i32, i32, u32, u32)>,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            always_on_top: false,
            balance_minutes: 10,
            quota_minutes: 3,
            huawei_site: "cn".into(),
            codex_path: String::new(),
            antigravity_port: 8045,
            antigravity_url: String::new(),
            antigravity_account: String::new(),
            antigravity_groups: vec![],
            antigravity_minutes: 5,
            desktop_mode: false,
            compact: true,
            live_background: false,
            glass: "frosted".into(),
            glass_tone: "light".into(),
            opacity: 65,
            font_scale: 100,
            position_locked: false,
            show_expiry: true,
            visible_platforms: vec!["deepseek".into(), "huawei".into(), "codex".into()],
            desktop_position: None,
            panel_bounds: None,
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Store {
    pub schema_version: u32,
    pub settings: Settings,
    pub snapshots: Vec<Snapshot>,
}
impl Default for Store {
    fn default() -> Self {
        Self {
            schema_version: 1,
            settings: Settings::default(),
            snapshots: vec![
                Snapshot::empty("deepseek", "unconfigured", "添加 API Key 后查看余额"),
                Snapshot::empty("huawei", "unconfigured", "添加 AK / SK 后查看代金券"),
                Snapshot::empty("codex", "idle", "等待读取本机登录额度"),
                Snapshot::empty(
                    "antigravity",
                    "unconfigured",
                    "连接本机 Antigravity Tools 后读取额度",
                ),
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn old_settings_gain_display_defaults_without_losing_account_configuration() {
        let settings:Settings=serde_json::from_str(r#"{"alwaysOnTop":true,"balanceMinutes":7,"quotaMinutes":4,"huaweiSite":"cn","codexPath":"C:\\Tools\\codex.exe"}"#).unwrap();
        assert!(settings.always_on_top);
        assert_eq!(settings.balance_minutes, 7);
        assert_eq!(settings.codex_path, "C:\\Tools\\codex.exe");
        assert!(!settings.desktop_mode);
        assert_eq!(settings.glass, "frosted");
        assert_eq!(settings.font_scale, 100);
        assert_eq!(settings.visible_platforms.len(), 3);
        assert_eq!(settings.antigravity_port, 8045);
        assert!(settings.antigravity_url.is_empty());
        assert_eq!(settings.antigravity_minutes, 5);
        assert!(settings.antigravity_account.is_empty());
    }
}
