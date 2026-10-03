use super::ProviderError;
use crate::model::{ProviderId, ProviderSnapshot, ProviderStatus, QuotaGroup, QuotaWindow};
use serde_json::{json, Map, Value};
use std::process::Stdio;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::time::{timeout, Duration};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);
const MAX_LINE_BYTES: usize = 1_048_576;

pub async fn fetch() -> Result<ProviderSnapshot, ProviderError> {
    let mut command = Command::new("cmd.exe");
    command
        .args([
            "/d",
            "/s",
            "/c",
            "codex.cmd",
            "app-server",
            "--listen",
            "stdio://",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    hide_console(&mut command);

    let mut child = command.spawn().map_err(|_| ProviderError::Command)?;
    let result = timeout(REQUEST_TIMEOUT, exchange(&mut child)).await;
    let _ = child.kill().await;
    let _ = child.wait().await;
    match result {
        Ok(result) => result,
        Err(_) => Err(ProviderError::Timeout),
    }
}

async fn exchange(child: &mut Child) -> Result<ProviderSnapshot, ProviderError> {
    let mut stdout = BufReader::new(child.stdout.take().ok_or(ProviderError::Io)?);
    let stdin = child.stdin.as_mut().ok_or(ProviderError::Io)?;

    send_json(
        stdin,
        json!({
            "id": 1,
            "method": "initialize",
            "params": {
                "clientInfo": {
                    "name": "ai_quota_mini",
                    "title": "AI Quota Mini",
                    "version": "0.1.0"
                }
            }
        }),
    )
    .await?;
    let initialize = read_response(&mut stdout, 1).await?;
    if initialize.get("error").is_some() {
        return Err(ProviderError::Unavailable(
            "Codex 帳號無法提供配額資料".to_string(),
        ));
    }

    send_json(stdin, json!({ "method": "initialized" })).await?;
    send_json(
        stdin,
        json!({ "id": 2, "method": "account/rateLimits/read" }),
    )
    .await?;
    let rate_limits = read_response(&mut stdout, 2).await?;
    if rate_limits.get("error").is_some() {
        return Err(ProviderError::Unavailable(
            "Codex 帳號目前沒有可讀取的配額".to_string(),
        ));
    }
    parse_codex_response(&rate_limits)
}

async fn send_json<W>(writer: &mut W, message: Value) -> Result<(), ProviderError>
where
    W: AsyncWrite + Unpin,
{
    let mut encoded = serde_json::to_vec(&message).map_err(|_| ProviderError::Io)?;
    encoded.push(b'\n');
    writer
        .write_all(&encoded)
        .await
        .map_err(|_| ProviderError::Io)?;
    writer.flush().await.map_err(|_| ProviderError::Io)
}

async fn read_response<R>(reader: &mut R, id: i64) -> Result<Value, ProviderError>
where
    R: AsyncBufRead + Unpin,
{
    let mut line = String::new();
    loop {
        line.clear();
        let bytes = reader
            .read_line(&mut line)
            .await
            .map_err(|_| ProviderError::Io)?;
        if bytes == 0 {
            return Err(ProviderError::Unavailable(
                "Codex 連線已中斷，無法取得配額".to_string(),
            ));
        }
        if bytes > MAX_LINE_BYTES {
            return Err(ProviderError::Parse("response line too large".to_string()));
        }
        let Ok(message) = serde_json::from_str::<Value>(line.trim()) else {
            continue;
        };
        if message.get("id").and_then(Value::as_i64) == Some(id) {
            return Ok(message);
        }
    }
}

pub fn parse_codex_response(response: &Value) -> Result<ProviderSnapshot, ProviderError> {
    let result = response.get("result").unwrap_or(response);
    let mut windows = result
        .get("rateLimitsByLimitId")
        .map(parse_windows)
        .unwrap_or_default();
    if windows.is_empty() {
        windows = result
            .get("rateLimits")
            .map(parse_windows)
            .unwrap_or_default();
    }

    if windows.is_empty() {
        return Err(ProviderError::Unavailable(
            "Codex 帳號目前沒有可讀取的配額".to_string(),
        ));
    }

    Ok(ProviderSnapshot {
        provider: ProviderId::Codex,
        status: ProviderStatus::Ok,
        message: None,
        updated_at: Some(chrono::Utc::now().timestamp()),
        stale: false,
        groups: vec![QuotaGroup {
            id: "codex".to_string(),
            title: "Codex".to_string(),
            windows,
        }],
    })
}

fn parse_windows(source: &Value) -> Vec<QuotaWindow> {
    let mut windows = Vec::new();
    for (key, value) in limit_entries(source) {
        let Some(object) = value.as_object() else {
            continue;
        };
        let limit_id = object
            .get("limitId")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty())
            .unwrap_or(key);
        let limit_name = object
            .get("limitName")
            .and_then(Value::as_str)
            .filter(|name| !name.is_empty());
        if let Some(window) = parse_window(limit_id, limit_name, "primary", object) {
            windows.push(window);
        }
        if let Some(window) = parse_window(limit_id, limit_name, "secondary", object) {
            windows.push(window);
        }
    }
    windows
}

fn limit_entries(source: &Value) -> Vec<(&str, &Value)> {
    let Some(object) = source.as_object() else {
        return Vec::new();
    };
    if object.contains_key("primary") || object.contains_key("secondary") {
        return vec![("codex", source)];
    }
    object
        .iter()
        .map(|(key, value)| (key.as_str(), value))
        .collect()
}

fn parse_window(
    limit_id: &str,
    limit_name: Option<&str>,
    field: &str,
    object: &Map<String, Value>,
) -> Option<QuotaWindow> {
    let window = object.get(field)?.as_object()?;
    let used = window.get("usedPercent")?.as_f64()?;
    if !used.is_finite() || !(0.0..=100.0).contains(&used) {
        return None;
    }
    let duration = window
        .get("windowDurationMins")
        .and_then(Value::as_u64)
        .filter(|duration| *duration > 0);
    let reset_at = window.get("resetsAt").and_then(|value| {
        value
            .as_i64()
            .or_else(|| value.as_u64().and_then(|number| i64::try_from(number).ok()))
    });
    let bucket_label = limit_name.unwrap_or(match field {
        "primary" => "主要配額",
        "secondary" => "次要配額",
        _ => field,
    });
    let label = match duration_label(duration) {
        Some(duration_label) => format!("{bucket_label} · {duration_label}"),
        None => bucket_label.to_string(),
    };
    Some(QuotaWindow {
        id: format!("{limit_id}-{field}"),
        label,
        remaining_percent: (100.0 - used).clamp(0.0, 100.0),
        window_duration_mins: duration,
        reset_at,
    })
}

fn duration_label(duration: Option<u64>) -> Option<&'static str> {
    match duration {
        Some(10_080) => Some("7 天"),
        Some(300) => Some("5 小時"),
        _ => None,
    }
}

fn hide_console(command: &mut Command) {
    #[cfg(windows)]
    command.creation_flags(0x0800_0000);
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_primary_and_secondary_from_limit_map() {
        let response = json!({
            "result": {"rateLimitsByLimitId": {
                "codex": {
                    "limitId": "codex",
                    "primary": {"usedPercent": 48, "windowDurationMins": 10080, "resetsAt": 1791047466},
                    "secondary": {"usedPercent": 12.5, "windowDurationMins": 300, "resetsAt": 1790000000}
                }
            }}
        });
        let snapshot = parse_codex_response(&response).unwrap();
        assert_eq!(snapshot.groups[0].windows.len(), 2);
        assert!((snapshot.groups[0].windows[0].remaining_percent - 52.0).abs() < f64::EPSILON);
        assert!((snapshot.groups[0].windows[1].remaining_percent - 87.5).abs() < f64::EPSILON);
        assert!(snapshot.groups[0].windows[0].label.contains("7 天"));
        assert!(snapshot.groups[0].windows[1].label.contains("5 小時"));
    }

    #[test]
    fn falls_back_when_limit_map_is_empty_but_rate_limits_is_valid() {
        let response = json!({
            "result": {
                "rateLimitsByLimitId": {},
                "rateLimits": {
                    "primary": {"usedPercent": 25, "windowDurationMins": 60}
                }
            }
        });
        let snapshot = parse_codex_response(&response).unwrap();
        assert_eq!(snapshot.groups[0].windows.len(), 1);
        assert_eq!(snapshot.groups[0].windows[0].remaining_percent, 75.0);
    }

    #[test]
    fn retains_multiple_limit_ids_and_names() {
        let response = json!({
            "result": {"rateLimitsByLimitId": {
                "fast": {"limitId": "fast", "limitName": "Fast models", "primary": {"usedPercent": 10, "windowDurationMins": 300}},
                "long": {"limitId": "long", "limitName": "Long models", "primary": {"usedPercent": 20, "windowDurationMins": 10080}}
            }}
        });
        let snapshot = parse_codex_response(&response).unwrap();
        let windows = &snapshot.groups[0].windows;
        assert_eq!(windows.len(), 2);
        assert!(windows
            .iter()
            .any(|window| window.id == "fast-primary" && window.label.contains("Fast models")));
        assert!(windows
            .iter()
            .any(|window| window.id == "long-primary" && window.label.contains("7 天")));
    }

    #[test]
    fn falls_back_to_rate_limits_and_skips_null_windows() {
        let response = json!({
            "result": {"rateLimits": {
                "primary": {"usedPercent": 0, "windowDurationMins": 60},
                "secondary": null
            }}
        });
        let snapshot = parse_codex_response(&response).unwrap();
        assert_eq!(snapshot.groups[0].windows.len(), 1);
        assert_eq!(snapshot.groups[0].windows[0].remaining_percent, 100.0);
    }

    #[test]
    fn reports_unavailable_when_quota_is_missing() {
        let response = json!({"result": {"rateLimitsByLimitId": null}});
        assert!(matches!(
            parse_codex_response(&response),
            Err(ProviderError::Unavailable(_))
        ));
    }
}
