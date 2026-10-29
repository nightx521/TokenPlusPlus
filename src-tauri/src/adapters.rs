//! Closed registry of supported, read-only quota adapters.
//! Adding metadata alone does not implement a new provider.
use crate::{
    antigravity,
    model::{Settings, Snapshot},
    providers,
};
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Adapter {
    Agent2Api,
    DeepSeek,
    Huawei,
    Codex,
    Antigravity,
}

pub const ALL: [Adapter; 5] = [
    Adapter::Agent2Api,
    Adapter::DeepSeek,
    Adapter::Huawei,
    Adapter::Codex,
    Adapter::Antigravity,
];

impl Adapter {
    pub fn parse(id: &str) -> Option<Self> {
        ALL.into_iter().find(|a| a.id() == id)
    }
    pub fn id(self) -> &'static str {
        match self {
            Self::Agent2Api => "agent2api",
            Self::DeepSeek => "deepseek",
            Self::Huawei => "huawei",
            Self::Codex => "codex",
            Self::Antigravity => "antigravity",
        }
    }
    pub fn needs_credential(self) -> bool {
        self != Self::Codex
    }
    pub fn interval_minutes(self, settings: &Settings) -> u64 {
        match self {
            Self::Agent2Api => settings.agent2api_minutes,
            Self::Codex => settings.quota_minutes,
            Self::Antigravity => settings.antigravity_minutes,
            _ => settings.balance_minutes,
        }
    }
    pub fn url(self) -> &'static str {
        match self {
            Self::DeepSeek => "https://platform.deepseek.com/usage",
            Self::Huawei => "https://account.huaweicloud.com/usercenter/#/coupons",
            Self::Agent2Api => "https://github.com/aimod-cc/Agent2API",
            Self::Codex => "https://chatgpt.com/codex/settings/usage",
            Self::Antigravity => "https://antigravity.google/",
        }
    }
    pub async fn query(
        self,
        client: &reqwest::Client,
        settings: &Settings,
        force: bool,
    ) -> Result<Snapshot, String> {
        match self {
            Self::DeepSeek => providers::deepseek(client).await,
            Self::Huawei => {
                tokio::time::timeout(Duration::from_secs(60), providers::huawei(client, settings))
                    .await
                    .unwrap_or_else(|_| Err("华为云完整查询超时，请稍后重试".into()))
            }
            Self::Agent2Api => crate::agent2api::query(settings, force).await,
            Self::Codex => providers::codex(settings).await,
            Self::Antigravity => antigravity::query(settings, force).await,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unknown_provider_never_falls_back_to_another_account() {
        for adapter in ALL {
            assert_eq!(Adapter::parse(adapter.id()), Some(adapter));
        }
        for id in ["", "unknown", "CODEX", "https://example.com"] {
            assert_eq!(Adapter::parse(id), None);
        }
        assert_eq!(ALL.into_iter().filter(|a| a.needs_credential()).count(), 4);
        assert!(!Adapter::Codex.needs_credential());
    }
    #[test]
    fn scheduling_respects_independent_quota_and_balance_preferences() {
        let settings = Settings {
            balance_minutes: 11,
            quota_minutes: 7,
            antigravity_minutes: 5,
            ..Settings::default()
        };
        assert_eq!(Adapter::DeepSeek.interval_minutes(&settings), 11);
        assert_eq!(Adapter::Huawei.interval_minutes(&settings), 11);
        assert_eq!(Adapter::Codex.interval_minutes(&settings), 7);
        assert_eq!(Adapter::Antigravity.interval_minutes(&settings), 5);
    }
}
