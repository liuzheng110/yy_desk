//! 应用基本信息命令。

/// 当前应用版本（来自 tauri.conf.json）。
#[tauri::command]
pub fn get_app_version(app: tauri::AppHandle) -> String {
    app.package_info().version.to_string()
}

/// 当前运行平台（windows / macos / linux）。
#[tauri::command]
pub fn get_platform() -> String {
    std::env::consts::OS.to_string()
}

/// 前端就绪后显式请求显示主窗口。
#[tauri::command]
pub fn show_main_window(app: tauri::AppHandle) -> bool {
    crate::show_main_window(&app)
}
