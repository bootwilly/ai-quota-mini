use super::ProviderError;
use crate::model::{ProviderId, ProviderSnapshot, ProviderStatus, QuotaGroup, QuotaWindow};
use serde_json::Value;
use std::path::PathBuf;
use std::time::Duration;

const USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";
const BETA_HEADER: &str = "oauth-2025-04-20";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);
const MAX_CREDENTIAL_BYTES: u64 = 65_536;

/// Reads Claude subscription quota (5-hour / weekly windows) using the OAuth
/// login that Claude Code already stored on this machine.
///
/// The usage endpoint is not publicly documented and may change. Any failure is
/// reported as a status; usage is never estimated from session cost or tokens.
/// The access token is only held in memory and is never logged or persisted.
pub async fn fetch() -> Result<ProviderSnapshot, ProviderError> {
    let token = load_access_token().await?;
    let body = request_usage(&token).await?;
    parse_usage_response(&body)
}

async fn load_access_token() -> Result<String, ProviderError> {
    let path = credentials_path().ok_or_else(|| {
        ProviderError::Auth("找不到 Claude Code 登入資料，請先執行 claude 登入".to_string())
    })?;
    let metadata = tokio::fs::metadata(&path).await.map_err(|_| {
        ProviderError::Auth("找不到 Claude Code 登入資料，請先執行 claude 登入".to_string())
    })?;
    if metadata.len() > MAX_CREDENTIAL_BYTES {
        return Err(ProviderError::Parse(
            "credentials file too large".to_string(),
        ));
    }
    let text = tokio::fs::read_to_string(&path)
        .await
        .map_err(|_| ProviderError::Io)?;
    parse_credentials(&text, chrono::Utc::now().timestamp_millis())
}

fn credentials_path() -> Option<PathBuf> {
    let base = std::env::var_os("CLAUDE_CONFIG_DIR")
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
        .or_else(|| {
            std::env::var_os("USERPROFILE")
                .or_else(|| std::env::var_os("HOME"))
                .map(|home| PathBuf::from(home).join(".claude"))
        })?;
    Some(base.join(".credentials.json"))
}

fn parse_credentials(text: &str, now_millis: i64) -> Result<String, ProviderError> {
    let value: Value = serde_json::from_str(text)
        .map_err(|_| ProviderError::Parse("credentials not valid json".to_string()))?;
    let oauth = value.get("claudeAiOauth").ok_or_else(|| {
        ProviderError::Auth(
            "Claude Code 尚未使用 Claude 帳號登入，請先執行 claude 登入".to_string(),
        )
    })?;
    let token = oauth
        .get("accessToken")
        .and_then(Value::as_str)
        .filter(|token| !token.is_empty())
        .ok_or_else(|| {
            ProviderError::Auth(
                "Claude Code 尚未使用 Claude 帳號登入，請先執行 claude 登入".to_string(),
            )
        })?;
    if let Some(expires_at) = oauth.get("expiresAt").and_then(Value::as_i64) {
        if expires_at <= now_millis {
            return Err(ProviderError::Auth(
                "Claude 登入已過期，請開啟一次 Claude Code 以更新登入".to_string(),
            ));
        }
    }
    Ok(token.to_string())
}

async fn request_usage(token: &str) -> Result<Value, ProviderError> {
    let client = reqwest::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .user_agent(concat!("ai-quota-mini/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|_| ProviderError::Io)?;
    let response = client
        .get(USAGE_URL)
        .bearer_auth(token)
        .header("anthropic-beta", BETA_HEADER)
        .header("accept", "application/json")
        .send()
        .await
        .map_err(|error| {
            if error.is_timeout() {
                ProviderError::Timeout
            } else {
                ProviderError::Unavailable("無法連線到 Claude 服務，請檢查網路".to_string())
            }
        })?;
    let status = response.status();
    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        return Err(ProviderError::Auth(
            "Claude 登入無效或已過期，請開啟 Claude Code 重新登入".to_string(),
        ));
    }
    if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
        return Err(ProviderError::Unavailable(
            "Claude 配額查詢過於頻繁，請稍後再試".to_string(),
        ));
    }
    if !status.is_success() {
        return Err(ProviderError::Unavailable(format!(
            "Claude 配額服務暫時無法使用（HTTP {}）",
            status.as_u16()
        )));
    }
    response
        .json::<Value>()
        .await
        .map_err(|_| ProviderError::Parse("usage response not valid json".to_string()))
}

const KNOWN_WINDOWS: [(&str, &str, Option<u64>); 4] = [
    ("five_hour", "5 小時", Some(300)),
    ("seven_day", "7 天", Some(10_080)),
    ("seven_day_opus", "Opus · 7 天", Some(10_080)),
    ("seven_day_sonnet", "Sonnet · 7 天", Some(10_080)),
];

pub fn parse_usage_response(response: &Value) -> Result<ProviderSnapshot, ProviderError> {
    let mut windows = Vec::new();
    for (key, label, duration) in KNOWN_WINDOWS {
        if let Some(window) = response
            .get(key)
            .and_then(|value| parse_window(key, label, duration, value))
        {
            windows.push(window);
        }
    }

    if windows.is_empty() {
        return Err(ProviderError::Unavailable(
            "Claude 帳號目前沒有可讀取的配額".to_string(),
        ));
    }

    Ok(ProviderSnapshot {
        provider: ProviderId::ClaudeCode,
        status: ProviderStatus::Ok,
        message: None,
        updated_at: Some(chrono::Utc::now().timestamp()),
        stale: false,
        groups: vec![QuotaGroup {
            id: "claude".to_string(),
            title: "Claude".to_string(),
            windows,
        }],
    })
}

fn parse_window(
    key: &str,
    label: &str,
    duration: Option<u64>,
    value: &Value,
) -> Option<QuotaWindow> {
    let object = value.as_object()?;
    let used = object.get("utilization")?.as_f64()?;
    if !used.is_finite() || !(0.0..=100.0).contains(&used) {
        return None;
    }
    let reset_at = object.get("resets_at").and_then(|value| match value {
        Value::String(text) => chrono::DateTime::parse_from_rfc3339(text)
            .ok()
            .map(|time| time.timestamp()),
        Value::Number(number) => number.as_i64(),
        _ => None,
    });
    Some(QuotaWindow {
        id: format!("claude-{key}"),
        label: label.to_string(),
        remaining_percent: (100.0 - used).clamp(0.0, 100.0),
        window_duration_mins: duration,
        reset_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_five_hour_and_weekly_windows() {
        let response = json!({
            "five_hour": {"utilization": 37.0, "resets_at": "2026-10-07T12:00:00.000000+00:00"},
            "seven_day": {"utilization": 26, "resets_at": "2026-10-10T03:00:00+00:00"},
            "seven_day_opus": null
        });
        let snapshot = parse_usage_response(&response).unwrap();
        let windows = &snapshot.groups[0].windows;
        assert_eq!(snapshot.provider, ProviderId::ClaudeCode);
        assert_eq!(windows.len(), 2);
        assert_eq!(windows[0].id, "claude-five_hour");
        assert!((windows[0].remaining_percent - 63.0).abs() < f64::EPSILON);
        assert_eq!(windows[0].window_duration_mins, Some(300));
        assert_eq!(windows[0].reset_at, Some(1_791_374_400));
        assert!((windows[1].remaining_percent - 74.0).abs() < f64::EPSILON);
        assert_eq!(windows[1].window_duration_mins, Some(10_080));
    }

    #[test]
    fn keeps_model_specific_weekly_windows() {
        let response = json!({
            "five_hour": {"utilization": 0, "resets_at": null},
            "seven_day": {"utilization": 100, "resets_at": null},
            "seven_day_opus": {"utilization": 50.5, "resets_at": null},
            "seven_day_sonnet": {"utilization": 10, "resets_at": null}
        });
        let snapshot = parse_usage_response(&response).unwrap();
        let windows = &snapshot.groups[0].windows;
        assert_eq!(windows.len(), 4);
        assert_eq!(windows[0].remaining_percent, 100.0);
        assert_eq!(windows[1].remaining_percent, 0.0);
        assert_eq!(windows[0].reset_at, None);
        assert!(windows[2].label.contains("Opus"));
    }

    #[test]
    fn skips_invalid_values_without_inventing_data() {
        let response = json!({
            "five_hour": {"utilization": -5},
            "seven_day": {"utilization": 140},
            "seven_day_opus": {"utilization": "n/a"},
            "seven_day_sonnet": {"resets_at": "2026-10-10T03:00:00+00:00"}
        });
        assert!(matches!(
            parse_usage_response(&response),
            Err(ProviderError::Unavailable(_))
        ));
    }

    #[test]
    fn reports_unavailable_for_unexpected_shapes() {
        for response in [json!({}), json!([]), json!(null), json!({"error": "x"})] {
            assert!(matches!(
                parse_usage_response(&response),
                Err(ProviderError::Unavailable(_))
            ));
        }
    }

    #[test]
    fn reads_token_from_valid_credentials() {
        let text = r#"{"claudeAiOauth":{"accessToken":"tok","refreshToken":"r","expiresAt":2000}}"#;
        assert_eq!(parse_credentials(text, 1000).unwrap(), "tok");
    }

    #[test]
    fn expired_credentials_ask_user_to_reopen_claude_code() {
        let text = r#"{"claudeAiOauth":{"accessToken":"tok","expiresAt":1000}}"#;
        assert!(matches!(
            parse_credentials(text, 1000),
            Err(ProviderError::Auth(_))
        ));
    }

    #[test]
    fn missing_oauth_or_token_is_auth_error() {
        for text in [
            r#"{}"#,
            r#"{"claudeAiOauth":{}}"#,
            r#"{"claudeAiOauth":{"accessToken":""}}"#,
        ] {
            assert!(matches!(
                parse_credentials(text, 0),
                Err(ProviderError::Auth(_))
            ));
        }
    }

    #[test]
    fn invalid_credentials_json_does_not_leak_content() {
        let error = parse_credentials("secret-not-json", 0).unwrap_err();
        assert!(!error.to_string().contains("secret"));
    }
}
