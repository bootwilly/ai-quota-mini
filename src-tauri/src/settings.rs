use crate::model::Settings;
use std::fs;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};

fn settings_path(app: &AppHandle) -> Result<PathBuf, String> {
    let directory = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("無法取得應用程式資料目錄：{error}"))?;
    Ok(directory.join("settings.json"))
}

pub fn load(app: &AppHandle) -> Result<Settings, String> {
    let path = settings_path(app)?;
    if !path.exists() {
        return Ok(Settings::default());
    }

    let bytes = fs::read(&path).map_err(|error| format!("讀取設定失敗：{error}"))?;
    let settings: Settings =
        serde_json::from_slice(&bytes).map_err(|error| format!("設定格式無效：{error}"))?;
    settings.validate()?;
    Ok(settings)
}

pub fn save(app: &AppHandle, settings: &Settings) -> Result<(), String> {
    settings.validate()?;
    let path = settings_path(app)?;
    let directory = path.parent().ok_or_else(|| "設定目錄無效".to_string())?;
    fs::create_dir_all(directory).map_err(|error| format!("建立設定目錄失敗：{error}"))?;

    let temporary_path = path.with_extension("json.tmp");
    let encoded =
        serde_json::to_vec_pretty(settings).map_err(|error| format!("序列化設定失敗：{error}"))?;
    fs::write(&temporary_path, encoded).map_err(|error| format!("寫入設定失敗：{error}"))?;
    replace_file(&temporary_path, &path).map_err(|error| format!("套用設定失敗：{error}"))
}

/// Replace the settings file without deleting the previous version first.
/// Windows uses MoveFileExW with replace/write-through flags so a failed
/// replacement leaves the existing settings file intact.
fn replace_file(temporary_path: &Path, destination: &Path) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Storage::FileSystem::{
            MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
        };

        let source: Vec<u16> = temporary_path
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let target: Vec<u16> = destination
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let flags = MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH;
        let replaced = unsafe { MoveFileExW(source.as_ptr(), target.as_ptr(), flags) };
        if replaced == 0 {
            return Err(std::io::Error::last_os_error());
        }
        return Ok(());
    }

    #[cfg(not(windows))]
    fs::rename(temporary_path, destination)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_directory() -> PathBuf {
        std::env::temp_dir().join(format!(
            "ai-quota-mini-settings-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock")
                .as_nanos()
        ))
    }

    #[test]
    fn validates_polling_bounds() {
        let mut settings = Settings::default();
        settings.polling_interval_seconds = Settings::MIN_POLLING_SECONDS;
        assert!(settings.validate().is_ok());
        settings.polling_interval_seconds = Settings::MAX_POLLING_SECONDS + 1;
        assert!(settings.validate().is_err());
    }

    #[test]
    fn selects_only_enabled_providers() {
        let settings = Settings {
            codex_enabled: false,
            antigravity_enabled: true,
            claude_code_enabled: true,
            ..Settings::default()
        };
        assert_eq!(settings.enabled_providers().len(), 2);
    }

    #[test]
    fn replacement_preserves_previous_file_when_source_is_missing() {
        let directory = test_directory();
        fs::create_dir_all(&directory).expect("create test directory");
        let destination = directory.join("settings.json");
        let missing_source = directory.join("missing.tmp");
        fs::write(&destination, b"previous settings").expect("write previous settings");

        assert!(replace_file(&missing_source, &destination).is_err());
        assert_eq!(
            fs::read(&destination).expect("read previous settings"),
            b"previous settings"
        );
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn replacement_commits_new_file_without_an_intermediate_delete() {
        let directory = test_directory();
        fs::create_dir_all(&directory).expect("create test directory");
        let destination = directory.join("settings.json");
        let temporary = directory.join("settings.json.tmp");
        fs::write(&destination, b"previous settings").expect("write previous settings");
        fs::write(&temporary, b"new settings").expect("write new settings");

        replace_file(&temporary, &destination).expect("replace settings");
        assert_eq!(
            fs::read(&destination).expect("read new settings"),
            b"new settings"
        );
        assert!(!temporary.exists());
        let _ = fs::remove_dir_all(directory);
    }
}
