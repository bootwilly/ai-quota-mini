mod antigravity;
mod claude_code;
mod codex;

use crate::model::{ProviderId, ProviderSnapshot, ProviderStatus};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProviderError {
    #[error("provider unavailable: {0}")]
    Unavailable(String),
    #[error("authentication unavailable: {0}")]
    Auth(String),
    #[error("provider timed out")]
    Timeout,
    #[error("provider command failed")]
    Command,
    #[error("provider response could not be parsed: {0}")]
    Parse(String),
    #[error("provider I/O failed")]
    Io,
}

pub async fn fetch(provider: ProviderId) -> Result<ProviderSnapshot, ProviderError> {
    match provider {
        ProviderId::Codex => codex::fetch().await,
        ProviderId::Antigravity => antigravity::fetch().await,
        ProviderId::ClaudeCode => claude_code::fetch().await,
    }
}

pub fn failure_snapshot(provider: ProviderId, error: &ProviderError) -> ProviderSnapshot {
    let status = match error {
        ProviderError::Auth(_) => ProviderStatus::Auth,
        ProviderError::Unavailable(_) => ProviderStatus::Unavailable,
        _ => ProviderStatus::Error,
    };
    ProviderSnapshot {
        provider,
        status,
        message: Some(user_message(error)),
        updated_at: None,
        stale: false,
        groups: Vec::new(),
    }
}

fn user_message(error: &ProviderError) -> String {
    match error {
        ProviderError::Auth(message) | ProviderError::Unavailable(message) => message.clone(),
        ProviderError::Timeout => "服務回應逾時，請稍後重試".to_string(),
        ProviderError::Command => "無法啟動服務命令，請確認 CLI 已安裝".to_string(),
        ProviderError::Parse(_) => "服務回傳格式無法辨識".to_string(),
        ProviderError::Io => "讀取服務回應失敗，請稍後重試".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hides_parser_details_from_user_message() {
        let snapshot = failure_snapshot(
            ProviderId::Codex,
            &ProviderError::Parse("raw backend detail".to_string()),
        );
        assert_eq!(snapshot.message.as_deref(), Some("服務回傳格式無法辨識"));
    }
}
