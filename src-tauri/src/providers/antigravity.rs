use super::ProviderError;
use crate::model::{ProviderId, ProviderSnapshot, ProviderStatus, QuotaGroup, QuotaWindow};
use chrono::DateTime;
use serde_json::Value;
use std::path::PathBuf;
use std::process::Stdio;
use tokio::process::Command;
use tokio::time::{timeout, Duration};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(25);

pub async fn fetch() -> Result<ProviderSnapshot, ProviderError> {
    let executable = resolve_executable();
    let mut command = Command::new(executable);
    command
        .args([
            "--print",
            "/usage",
            "--output-format",
            "json",
            "--print-timeout",
            "15s",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    hide_console(&mut command);

    let output = timeout(REQUEST_TIMEOUT, command.output())
        .await
        .map_err(|_| ProviderError::Timeout)?
        .map_err(|_| ProviderError::Command)?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_ascii_lowercase();
        if stderr.contains("auth") || stderr.contains("login") || stderr.contains("sign in") {
            return Err(ProviderError::Auth(
                "Antigravity 尚未登入，請先完成 CLI 登入".to_string(),
            ));
        }
        return Err(ProviderError::Unavailable(
            "Antigravity 帳號無法提供配額資料，請確認已登入".to_string(),
        ));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_usage(&stdout)
}

fn resolve_executable() -> PathBuf {
    if let Some(local_app_data) = std::env::var_os("LOCALAPPDATA") {
        let known_path = PathBuf::from(local_app_data)
            .join("agy")
            .join("bin")
            .join("agy.exe");
        if known_path.is_file() {
            return known_path;
        }
    }
    PathBuf::from("agy.exe")
}

pub fn parse_usage(raw: &str) -> Result<ProviderSnapshot, ProviderError> {
    let value = parse_json_candidate(raw)
        .ok_or_else(|| ProviderError::Parse("usage response was not JSON".to_string()))?;
    let groups = find_groups(&value).ok_or_else(|| {
        ProviderError::Unavailable("Antigravity 尚未回傳可讀取的配額群組".to_string())
    })?;

    let mut parsed_groups = Vec::new();
    for (group_index, group) in groups.iter().enumerate() {
        let Some(object) = group.as_object() else {
            continue;
        };
        let title = object
            .get("name")
            .and_then(Value::as_str)
            .filter(|name| !name.trim().is_empty())
            .unwrap_or("模型群組")
            .to_string();
        let group_id = object
            .get("id")
            .and_then(Value::as_str)
            .filter(|id| !id.trim().is_empty())
            .unwrap_or("group");
        let Some(buckets) = object.get("buckets").and_then(Value::as_array) else {
            continue;
        };

        let mut windows = Vec::new();
        for (bucket_index, bucket) in buckets.iter().enumerate() {
            let Some(bucket_object) = bucket.as_object() else {
                continue;
            };
            let Some(fraction) = bucket_object
                .get("remaining_fraction")
                .and_then(Value::as_f64)
            else {
                continue;
            };
            if !fraction.is_finite() || !(0.0..=1.0).contains(&fraction) {
                continue;
            }
            let reset_at = match bucket_object.get("reset_time") {
                Some(Value::String(value)) => match DateTime::parse_from_rfc3339(value) {
                    Ok(parsed) => Some(parsed.timestamp()),
                    Err(_) => continue,
                },
                Some(Value::Null) | None => None,
                Some(_) => continue,
            };
            let window = bucket_object
                .get("window")
                .and_then(Value::as_str)
                .unwrap_or("quota");
            let label = match window {
                "weekly" => "一週視窗",
                "5h" => "5 小時視窗",
                other => other,
            };
            let id = bucket_object
                .get("id")
                .and_then(Value::as_str)
                .filter(|id| !id.trim().is_empty())
                .map(ToString::to_string)
                .unwrap_or_else(|| format!("{group_id}-{window}-{bucket_index}"));
            windows.push(QuotaWindow {
                id,
                label: label.to_string(),
                remaining_percent: fraction * 100.0,
                window_duration_mins: Some(window_duration_minutes(window)),
                reset_at,
            });
        }
        if !windows.is_empty() {
            parsed_groups.push(QuotaGroup {
                id: format!("{group_id}-{group_index}"),
                title,
                windows,
            });
        }
    }

    if parsed_groups.is_empty() {
        return Err(ProviderError::Unavailable(
            "Antigravity 尚未回傳可讀取的配額".to_string(),
        ));
    }

    Ok(ProviderSnapshot {
        provider: ProviderId::Antigravity,
        status: ProviderStatus::Ok,
        message: None,
        updated_at: Some(chrono::Utc::now().timestamp()),
        stale: false,
        groups: parsed_groups,
    })
}

fn window_duration_minutes(window: &str) -> u64 {
    match window {
        "weekly" => 10_080,
        "5h" => 300,
        _ => 0,
    }
}

fn parse_json_candidate(raw: &str) -> Option<Value> {
    if let Ok(value) = serde_json::from_str::<Value>(raw.trim()) {
        return Some(value);
    }
    raw.lines()
        .rev()
        .filter_map(|line| serde_json::from_str::<Value>(line.trim()).ok())
        .find(|value| find_groups(value).is_some())
}

fn find_groups(value: &Value) -> Option<&Vec<Value>> {
    match value {
        Value::Object(object) => {
            if let Some(groups) = object.get("groups").and_then(Value::as_array) {
                return Some(groups);
            }
            object.values().find_map(find_groups)
        }
        Value::Array(values) => values.iter().find_map(find_groups),
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
    fn parses_multiple_groups_and_fractional_windows() {
        let response = json!({"command": {"data": {"groups": [
            {"id": "gemini", "name": "Gemini Models", "buckets": [
                {"id": "g-week", "window": "weekly", "remaining_fraction": 0.42, "reset_time": "2026-10-02T01:02:03Z"},
                {"id": "g-five", "window": "5h", "remaining_fraction": 1.0, "reset_time": "2026-10-02T01:02:03Z"}
            ]},
            {"id": "claude", "name": "Claude and GPT models", "buckets": [
                {"id": "c-week", "window": "weekly", "remaining_fraction": 0.0, "reset_time": "2026-10-02T01:02:03Z"}
            ]}
        ]}}});
        let snapshot = parse_usage(&response.to_string()).unwrap();
        assert_eq!(snapshot.groups.len(), 2);
        assert_eq!(snapshot.groups[0].windows[0].remaining_percent, 42.0);
        assert_eq!(
            snapshot.groups[0].windows[1].window_duration_mins,
            Some(300)
        );
        assert_eq!(snapshot.groups[1].windows[0].remaining_percent, 0.0);
    }

    #[test]
    fn skips_null_missing_and_invalid_buckets() {
        let response = json!({"data": {"groups": [
            {"name": "Gemini Models", "buckets": null},
            {"name": "Claude and GPT models", "buckets": [
                null,
                {},
                {"remaining_fraction": 1.4, "window": "weekly"},
                {"remaining_fraction": 0.5, "window": "weekly", "reset_time": "not-a-date"},
                {"remaining_fraction": 0.5, "window": "weekly"}
            ]}
        ]}});
        let snapshot = parse_usage(&response.to_string()).unwrap();
        assert_eq!(snapshot.groups.len(), 1);
        assert_eq!(snapshot.groups[0].windows.len(), 1);
        assert_eq!(snapshot.groups[0].windows[0].remaining_percent, 50.0);
    }

    #[test]
    fn reports_unavailable_for_null_or_missing_groups() {
        assert!(matches!(
            parse_usage(r#"{"command":{"data":{"groups":null}}}"#),
            Err(ProviderError::Unavailable(_))
        ));
        assert!(matches!(
            parse_usage("{}"),
            Err(ProviderError::Unavailable(_))
        ));
    }
}
