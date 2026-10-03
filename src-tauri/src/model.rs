use serde::{Deserialize, Deserializer, Serialize};

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub enum ProviderId {
    Codex,
    Antigravity,
    ClaudeCode,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ProviderStatus {
    Loading,
    Ok,
    Error,
    Auth,
    Unavailable,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DisplayMode {
    Bar,
    Gauge,
}

impl Default for DisplayMode {
    fn default() -> Self {
        Self::Bar
    }
}

impl<'de> Deserialize<'de> for DisplayMode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = serde_json::Value::deserialize(deserializer)?;
        Ok(match value.as_str() {
            Some("gauge") => Self::Gauge,
            _ => Self::Bar,
        })
    }
}

fn default_display_mode() -> DisplayMode {
    DisplayMode::Bar
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub codex_enabled: bool,
    pub antigravity_enabled: bool,
    pub claude_code_enabled: bool,
    pub polling_interval_seconds: u32,
    pub always_on_top: bool,
    #[serde(default = "default_display_mode")]
    pub display_mode: DisplayMode,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            codex_enabled: true,
            antigravity_enabled: true,
            claude_code_enabled: false,
            polling_interval_seconds: 300,
            always_on_top: true,
            display_mode: DisplayMode::Bar,
        }
    }
}

impl Settings {
    pub const MIN_POLLING_SECONDS: u32 = 30;
    pub const MAX_POLLING_SECONDS: u32 = 3_600;

    pub fn validate(&self) -> Result<(), String> {
        if !(Self::MIN_POLLING_SECONDS..=Self::MAX_POLLING_SECONDS)
            .contains(&self.polling_interval_seconds)
        {
            return Err(format!(
                "輪詢間隔必須介於 {} 到 {} 秒",
                Self::MIN_POLLING_SECONDS,
                Self::MAX_POLLING_SECONDS
            ));
        }
        Ok(())
    }

    pub fn enabled_providers(&self) -> Vec<ProviderId> {
        let mut providers = Vec::with_capacity(3);
        if self.codex_enabled {
            providers.push(ProviderId::Codex);
        }
        if self.antigravity_enabled {
            providers.push(ProviderId::Antigravity);
        }
        if self.claude_code_enabled {
            providers.push(ProviderId::ClaudeCode);
        }
        providers
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct QuotaWindow {
    pub id: String,
    pub label: String,
    pub remaining_percent: f64,
    pub window_duration_mins: Option<u64>,
    pub reset_at: Option<i64>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct QuotaGroup {
    pub id: String,
    pub title: String,
    pub windows: Vec<QuotaWindow>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderSnapshot {
    pub provider: ProviderId,
    pub status: ProviderStatus,
    pub message: Option<String>,
    pub updated_at: Option<i64>,
    pub stale: bool,
    pub groups: Vec<QuotaGroup>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_settings_without_display_mode_use_bar() {
        let settings: Settings = serde_json::from_str(
            r#"{
                "codexEnabled": true,
                "antigravityEnabled": false,
                "claudeCodeEnabled": false,
                "pollingIntervalSeconds": 300,
                "alwaysOnTop": true
            }"#,
        )
        .expect("legacy settings should deserialize");

        assert_eq!(settings.display_mode, DisplayMode::Bar);
        assert!(settings.codex_enabled);
    }

    #[test]
    fn unknown_display_mode_falls_back_to_bar_without_losing_settings() {
        let settings: Settings = serde_json::from_str(
            r#"{
                "codexEnabled": false,
                "antigravityEnabled": true,
                "claudeCodeEnabled": true,
                "pollingIntervalSeconds": 120,
                "alwaysOnTop": false,
                "displayMode": "future-mode"
            }"#,
        )
        .expect("unknown display mode should deserialize");

        assert_eq!(settings.display_mode, DisplayMode::Bar);
        assert!(!settings.codex_enabled);
        assert!(settings.antigravity_enabled);
        assert!(settings.claude_code_enabled);
    }
}
