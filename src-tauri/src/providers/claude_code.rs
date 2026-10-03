use super::ProviderError;
use crate::model::ProviderId;

/// Claude Code exposes authentication status, but the installed CLI does not
/// expose a documented account-quota read endpoint. Session usage/cost fields
/// are deliberately not interpreted as remaining account quota.
pub async fn fetch() -> Result<crate::model::ProviderSnapshot, ProviderError> {
    Err(ProviderError::Unavailable(
        "Claude Code 目前沒有可驗證的帳號配額來源；未使用 session 用量或成本推估".to_string(),
    ))
}

#[allow(dead_code)]
const PROVIDER: ProviderId = ProviderId::ClaudeCode;
