//! 应用设置命令。
//!
//! 裁剪自 dbx 的 app_settings.rs：保留桌面设置/外观/语言/关闭行为/
//! 标签页状态/调试日志等框架级设置；删除 AI、MCP、驱动目录、
//! 传输任务等数据库业务设置。

use std::{
    collections::HashSet,
    path::PathBuf,
    sync::{Arc, Mutex, OnceLock},
};

use app_core::storage::{AppAppearanceSettings, AppAppearanceSettingsPatch, DesktopSettings};
use tauri::{AppHandle, Manager, State, Window};

use super::state::AppState;
use crate::{
    apply_debug_log_level, apply_desktop_settings, clear_startup_probe_after_frontend_ready,
    hide_main_window_for_close, refresh_native_menus, request_app_close, AppLocaleState, CloseBehaviorState,
};

const DEVELOPMENT_OPEN_TABS_STATE_KEY: &str = "development_open_tabs";
const DETACHED_TABS_STATE_KEY: &str = "detached_tabs";
const EDITOR_SETTINGS_KEY: &str = "editor_settings";
static DETACHED_TABS_LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
static APPROVED_DETACHED_CLOSES: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

fn detached_tabs_lock() -> &'static tokio::sync::Mutex<()> {
    DETACHED_TABS_LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

fn approved_detached_closes() -> &'static Mutex<HashSet<String>> {
    APPROVED_DETACHED_CLOSES.get_or_init(|| Mutex::new(HashSet::new()))
}

pub(crate) fn take_approved_detached_window_close(label: &str) -> bool {
    approved_detached_closes()
        .lock()
        .map(|mut labels| labels.remove(label))
        .unwrap_or(false)
}

async fn load_detached_tabs_map(state: &AppState) -> Result<serde_json::Map<String, serde_json::Value>, String> {
    Ok(state
        .storage
        .load_open_tabs_state_with_key(DETACHED_TABS_STATE_KEY)
        .await?
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default())
}

fn open_tabs_state_key(debug_build: bool) -> &'static str {
    if debug_build {
        DEVELOPMENT_OPEN_TABS_STATE_KEY
    } else {
        "open_tabs"
    }
}

// ---- 桌面设置 ----

#[tauri::command]
pub async fn load_desktop_settings(state: State<'_, Arc<AppState>>) -> Result<DesktopSettings, String> {
    state.storage.load_desktop_settings().await
}

#[tauri::command]
pub async fn save_desktop_settings(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    settings: DesktopSettings,
) -> Result<(), String> {
    state.storage.save_desktop_settings(&settings).await?;
    apply_debug_log_level(settings.debug_logging_enabled);
    if let Err(err) = apply_desktop_settings(&app, &settings) {
        eprintln!("Failed to apply desktop settings: {err}");
    }
    Ok(())
}

// ---- 外观 ----

#[tauri::command]
pub async fn load_app_appearance_settings(state: State<'_, Arc<AppState>>) -> Result<AppAppearanceSettings, String> {
    state.storage.load_app_appearance_settings().await
}

#[tauri::command]
pub async fn update_app_appearance_settings(
    state: State<'_, Arc<AppState>>,
    patch: AppAppearanceSettingsPatch,
) -> Result<(), String> {
    state.storage.update_app_appearance_settings(&patch).await
}

#[tauri::command]
pub async fn set_app_locale(app: AppHandle, locale: String) -> Result<(), String> {
    if let Some(state) = app.try_state::<AppLocaleState>() {
        state.set(locale);
    }
    refresh_native_menus(&app).map_err(|e| e.to_string())
}

// ---- 关闭行为 ----

#[tauri::command]
pub async fn complete_app_close(app: AppHandle, window: Window, action: String) -> Result<(), String> {
    match action.as_str() {
        "quit" => {
            if let Some(state) = app.try_state::<CloseBehaviorState>() {
                state.allow_next_exit();
            }
            #[cfg(target_os = "macos")]
            {
                // 先整体移出屏幕，避免进程退出时 WKWebView 拆解在屏幕上闪红一帧。
                let _ = window.hide();
                let _ = app.hide();
                tokio::time::sleep(std::time::Duration::from_millis(250)).await;
            }
            app.exit(0);
        }
        "hide" => {
            hide_main_window_for_close(&app, &window);
        }
        _ => return Err(format!("unsupported close action: {action}")),
    }
    Ok(())
}

#[tauri::command]
pub fn mark_frontend_ready(app: AppHandle) -> Result<(), String> {
    let state = app
        .try_state::<CloseBehaviorState>()
        .ok_or_else(|| "close behavior state is unavailable".to_string())?;
    state.set_frontend_ready(true);
    let main_window_visible = app
        .get_webview_window("main")
        .is_some_and(|window| window.is_visible().unwrap_or(false));
    clear_startup_probe_after_frontend_ready(main_window_visible);
    Ok(())
}

#[tauri::command]
pub async fn request_app_close_from_window_controls(app: AppHandle) -> Result<(), String> {
    request_app_close(&app, "settings");
    Ok(())
}

// ---- 通用 JSON 设置（示例：编辑器设置）----

#[tauri::command]
pub async fn load_editor_settings(state: State<'_, Arc<AppState>>) -> Result<Option<serde_json::Value>, String> {
    state.storage.get_json(EDITOR_SETTINGS_KEY).await
}

#[tauri::command]
pub async fn save_editor_settings(state: State<'_, Arc<AppState>>, settings: serde_json::Value) -> Result<(), String> {
    state.storage.set_json(EDITOR_SETTINGS_KEY, &settings).await
}

// ---- 标签页 / 分离窗口状态 ----

#[tauri::command]
pub async fn load_open_tabs_state(state: State<'_, Arc<AppState>>) -> Result<Option<serde_json::Value>, String> {
    state
        .storage
        .load_open_tabs_state_with_key(open_tabs_state_key(cfg!(debug_assertions)))
        .await
}

#[tauri::command]
pub async fn save_open_tabs_state(state: State<'_, Arc<AppState>>, payload: serde_json::Value) -> Result<(), String> {
    state
        .storage
        .save_open_tabs_state_with_key(open_tabs_state_key(cfg!(debug_assertions)), &payload)
        .await
}

#[tauri::command]
pub async fn save_detached_tab_handoff(
    state: State<'_, Arc<AppState>>,
    tab_id: String,
    handoff: serde_json::Value,
) -> Result<(), String> {
    let _guard = detached_tabs_lock().lock().await;
    let mut tabs = load_detached_tabs_map(state.inner()).await?;
    tabs.insert(tab_id, handoff);
    state
        .storage
        .save_open_tabs_state_with_key(DETACHED_TABS_STATE_KEY, &serde_json::Value::Object(tabs))
        .await
}

#[tauri::command]
pub async fn load_detached_tab_handoff(
    state: State<'_, Arc<AppState>>,
    tab_id: String,
) -> Result<Option<serde_json::Value>, String> {
    let _guard = detached_tabs_lock().lock().await;
    Ok(load_detached_tabs_map(state.inner()).await?.remove(&tab_id))
}

#[tauri::command]
pub async fn list_detached_tab_handoffs(state: State<'_, Arc<AppState>>) -> Result<Vec<serde_json::Value>, String> {
    let _guard = detached_tabs_lock().lock().await;
    Ok(load_detached_tabs_map(state.inner()).await?.into_values().collect())
}

#[tauri::command]
pub async fn delete_detached_tab_handoff(state: State<'_, Arc<AppState>>, tab_id: String) -> Result<(), String> {
    let _guard = detached_tabs_lock().lock().await;
    let mut tabs = load_detached_tabs_map(state.inner()).await?;
    tabs.remove(&tab_id);
    state
        .storage
        .save_open_tabs_state_with_key(DETACHED_TABS_STATE_KEY, &serde_json::Value::Object(tabs))
        .await
}

#[tauri::command]
pub fn approve_detached_window_close(window: Window) -> Result<(), String> {
    if !window.label().starts_with("detached-tab-") {
        return Err("current window is not a detached tab window".to_string());
    }
    approved_detached_closes()
        .lock()
        .map_err(|_| "detached window close state is unavailable".to_string())?
        .insert(window.label().to_string());
    Ok(())
}

// ---- 调试日志 ----

#[tauri::command]
pub async fn load_native_debug_logs(app: AppHandle) -> Result<String, String> {
    let log_dir = app.path().app_log_dir().map_err(|e| e.to_string())?;
    tauri::async_runtime::spawn_blocking(move || load_native_debug_logs_from_dir(log_dir))
        .await
        .map_err(|err| err.to_string())?
}

fn load_native_debug_logs_from_dir(log_dir: PathBuf) -> Result<String, String> {
    const MAX_FILES: usize = 6;
    const MAX_FILE_BYTES: u64 = 512 * 1024;
    if !log_dir.exists() {
        return Ok(format!("Native log dir does not exist yet: {}", log_dir.display()));
    }
    let mut files = std::fs::read_dir(&log_dir)
        .map_err(|e| e.to_string())?
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let path = entry.path();
            let metadata = entry.metadata().ok()?;
            if !metadata.is_file() {
                return None;
            }
            let modified = metadata.modified().ok()?;
            Some((path, modified, metadata.len()))
        })
        .collect::<Vec<_>>();
    files.sort_by_key(|(_, modified, _)| *modified);
    files.reverse();

    let mut output = String::new();
    output.push_str(&format!("Native log dir: {}\n", log_dir.display()));
    for (path, _, len) in files.into_iter().take(MAX_FILES) {
        let name = path.file_name().and_then(|name| name.to_str()).unwrap_or("unknown");
        output.push_str(&format!("\n===== {name} =====\n"));
        let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
        let start = if len > MAX_FILE_BYTES {
            bytes.len().saturating_sub(MAX_FILE_BYTES as usize)
        } else {
            0
        };
        if start > 0 {
            output.push_str("[truncated to last 512 KiB]\n");
        }
        output.push_str(&String::from_utf8_lossy(&bytes[start..]));
    }
    Ok(output)
}
