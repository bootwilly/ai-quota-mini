mod model;
mod providers;
mod settings;

use futures::future::join_all;
use model::{ProviderId, ProviderSnapshot, ProviderStatus, Settings};
use providers::{failure_snapshot, fetch};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, LogicalSize, Manager, PhysicalSize, Size, State, WindowEvent};

const DEFAULT_WINDOW_WIDTH: f64 = 430.0;
const DEFAULT_EXPANDED_HEIGHT: f64 = 620.0;
const DEFAULT_COMPACT_WINDOW_HEIGHT: f64 = 230.0;
const MIN_COMPACT_WINDOW_HEIGHT: f64 = 180.0;
const MAX_COMPACT_WINDOW_HEIGHT: f64 = 4_000.0;

#[derive(Clone, Copy)]
struct WindowSize {
    width: u32,
    height: u32,
}

pub struct AppState {
    settings: Mutex<Settings>,
    snapshots: Mutex<HashMap<ProviderId, ProviderSnapshot>>,
    polling: AtomicBool,
    quit_requested: AtomicBool,
    expanded_size: Mutex<Option<WindowSize>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            settings: Mutex::new(Settings::default()),
            snapshots: Mutex::new(HashMap::new()),
            polling: AtomicBool::new(false),
            quit_requested: AtomicBool::new(false),
            expanded_size: Mutex::new(None),
        }
    }
}

#[tauri::command]
fn get_settings(state: State<'_, AppState>) -> Result<Settings, String> {
    state
        .settings
        .lock()
        .map(|settings| settings.clone())
        .map_err(|_| "讀取設定失敗".to_string())
}

#[tauri::command]
fn save_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    settings: Settings,
) -> Result<Settings, String> {
    settings.validate()?;
    settings::save(&app, &settings)?;
    {
        let mut current = state
            .settings
            .lock()
            .map_err(|_| "更新設定失敗".to_string())?;
        *current = settings.clone();
    }
    if let Some(window) = app.get_webview_window("main") {
        window
            .set_always_on_top(settings.always_on_top)
            .map_err(|error| format!("設定視窗置頂失敗：{error}"))?;
    }
    Ok(settings)
}

#[tauri::command]
async fn refresh_quotas(state: State<'_, AppState>) -> Result<Vec<ProviderSnapshot>, String> {
    if state.polling.swap(true, Ordering::AcqRel) {
        return Err("正在更新配額，請稍候".to_string());
    }

    let result = refresh_quotas_inner(&state).await;
    state.polling.store(false, Ordering::Release);
    result
}

async fn refresh_quotas_inner(state: &AppState) -> Result<Vec<ProviderSnapshot>, String> {
    let enabled = state
        .settings
        .lock()
        .map_err(|_| "讀取設定失敗".to_string())?
        .enabled_providers();

    let results = join_all(enabled.iter().copied().map(|provider| async move {
        let result = fetch(provider).await;
        (provider, result)
    }))
    .await;

    let mut snapshots = state
        .snapshots
        .lock()
        .map_err(|_| "更新配額狀態失敗".to_string())?;
    for (provider, result) in results {
        match result {
            Ok(snapshot) => {
                snapshots.insert(provider, snapshot);
            }
            Err(error) => {
                let failed = failure_snapshot(provider, &error);
                let snapshot = match snapshots.get(&provider) {
                    Some(previous) if !previous.groups.is_empty() => ProviderSnapshot {
                        provider,
                        status: ProviderStatus::Error,
                        message: failed.message,
                        updated_at: previous.updated_at,
                        stale: true,
                        groups: previous.groups.clone(),
                    },
                    _ => failed,
                };
                snapshots.insert(provider, snapshot);
            }
        }
    }

    Ok(enabled
        .iter()
        .filter_map(|provider| snapshots.get(provider).cloned())
        .collect())
}

#[tauri::command]
fn set_compact_mode(
    app: AppHandle,
    state: State<'_, AppState>,
    compact: bool,
    compact_height: Option<f64>,
) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "找不到主視窗".to_string())?;

    if compact {
        let current = window
            .inner_size()
            .map_err(|error| format!("讀取視窗尺寸失敗：{error}"))?;
        let mut expanded_size = state
            .expanded_size
            .lock()
            .map_err(|_| "儲存視窗尺寸失敗".to_string())?;
        if expanded_size.is_none() {
            *expanded_size = Some(WindowSize {
                width: current.width,
                height: current.height,
            });
        }
        drop(expanded_size);
        let target_height = compact_height
            .filter(|height| height.is_finite())
            .unwrap_or(DEFAULT_COMPACT_WINDOW_HEIGHT)
            .clamp(MIN_COMPACT_WINDOW_HEIGHT, MAX_COMPACT_WINDOW_HEIGHT);
        window
            .set_size(Size::Logical(LogicalSize::new(
                DEFAULT_WINDOW_WIDTH,
                target_height,
            )))
            .map_err(|error| format!("縮小視窗失敗：{error}"))?;
        return Ok(());
    }

    let target = state
        .expanded_size
        .lock()
        .map_err(|_| "讀取視窗尺寸失敗".to_string())?
        .unwrap_or(WindowSize {
            width: DEFAULT_WINDOW_WIDTH as u32,
            height: DEFAULT_EXPANDED_HEIGHT as u32,
        });
    window
        .set_size(Size::Physical(PhysicalSize::new(
            target.width,
            target.height,
        )))
        .map_err(|error| format!("還原視窗失敗：{error}"))?;
    *state
        .expanded_size
        .lock()
        .map_err(|_| "更新視窗尺寸失敗".to_string())? = None;
    Ok(())
}

#[tauri::command]
fn set_always_on_top(
    app: AppHandle,
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<(), String> {
    let mut settings = state
        .settings
        .lock()
        .map_err(|_| "更新設定失敗".to_string())?
        .clone();
    settings.always_on_top = enabled;
    settings::save(&app, &settings)?;
    *state
        .settings
        .lock()
        .map_err(|_| "更新設定失敗".to_string())? = settings;
    if let Some(window) = app.get_webview_window("main") {
        window
            .set_always_on_top(enabled)
            .map_err(|error| format!("設定視窗置頂失敗：{error}"))?;
    }
    Ok(())
}

#[tauri::command]
fn hide_to_tray(app: AppHandle) -> Result<(), String> {
    app.get_webview_window("main")
        .ok_or_else(|| "找不到主視窗".to_string())?
        .hide()
        .map_err(|error| format!("隱藏視窗失敗：{error}"))
}

#[tauri::command]
fn show_window(app: AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "找不到主視窗".to_string())?;
    window
        .show()
        .and_then(|_| window.set_focus())
        .map_err(|error| format!("顯示視窗失敗：{error}"))
}

#[tauri::command]
fn quit_app(app: AppHandle, state: State<'_, AppState>) {
    state.quit_requested.store(true, Ordering::Release);
    app.exit(0);
}

fn apply_initial_window_settings(app: &AppHandle, settings: &Settings) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("main") {
        window
            .set_always_on_top(settings.always_on_top)
            .map_err(|error| format!("套用視窗設定失敗：{error}"))?;
    }
    Ok(())
}

fn setup_tray(app: &tauri::App) -> tauri::Result<()> {
    use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
    use tauri::tray::TrayIconBuilder;

    let show = MenuItem::with_id(app, "show", "顯示視窗", true, None::<&str>)?;
    let refresh = MenuItem::with_id(app, "refresh", "重新整理配額", true, None::<&str>)?;
    let hide = MenuItem::with_id(app, "hide", "最小化到系統匣", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", "退出 AI Quota Mini", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &refresh, &hide, &separator, &quit])?;

    let mut builder = TrayIconBuilder::with_id("main-tray")
        .menu(&menu)
        .tooltip("AI Quota Mini")
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => {
                let _ = show_window(app.clone());
            }
            "refresh" => {
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    let state = app.state::<AppState>();
                    match refresh_quotas(state).await {
                        Ok(snapshots) => {
                            let _ = app.emit("quotas-updated", snapshots);
                        }
                        Err(error) => {
                            let _ = app.emit("quota-refresh-error", error);
                        }
                    }
                });
            }
            "hide" => {
                let _ = hide_to_tray(app.clone());
            }
            "quit" => {
                let state = app.state::<AppState>();
                state.quit_requested.store(true, Ordering::Release);
                app.exit(0);
            }
            _ => {}
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    Ok(())
}

pub fn run() {
    tauri::Builder::default()
        .manage(AppState::default())
        .setup(|app| {
            let loaded = settings::load(&app.handle()).unwrap_or_default();
            *app.state::<AppState>()
                .settings
                .lock()
                .map_err(|_| "初始化設定失敗")? = loaded.clone();
            apply_initial_window_settings(&app.handle(), &loaded).map_err(|error| {
                Box::new(std::io::Error::other(error)) as Box<dyn std::error::Error>
            })?;
            setup_tray(app)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let state = window.app_handle().state::<AppState>();
                if !state.quit_requested.load(Ordering::Acquire) {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_settings,
            save_settings,
            refresh_quotas,
            set_compact_mode,
            set_always_on_top,
            hide_to_tray,
            show_window,
            quit_app
        ])
        .run(tauri::generate_context!())
        .expect("AI Quota Mini 啟動失敗");
}
