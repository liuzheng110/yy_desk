#[tauri::command]
pub async fn list_system_fonts() -> Result<Vec<String>, String> {
    tauri::async_runtime::spawn_blocking(|| Ok(app_core::fonts::list_system_fonts()))
        .await
        .map_err(|err| err.to_string())?
}
