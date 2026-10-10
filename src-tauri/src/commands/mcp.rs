//! MCP 网关管理命令。
//!
//! - `mcp_status`：网关端口、服务器列表、各自缓存的工具数（供界面/CLI 展示）
//! - `mcp_reload_config`：重新加载 `data/mcp-servers.json`
//! - `mcp_refresh_tools`：逐个连接外部 MCP 扫描工具清单，写入持久化缓存

use serde_json::Value;
use tauri::{AppHandle, Manager, State};

use app_mcp::gateway::GatewayHandle;
use app_mcp::McpRegistryConfig;

#[tauri::command]
pub async fn mcp_status(gateway: State<'_, GatewayHandle>) -> Result<Value, String> {
    Ok(gateway.server_status().await)
}

#[tauri::command]
pub async fn mcp_reload_config(gateway: State<'_, GatewayHandle>) -> Result<Value, String> {
    gateway.reload().await.map_err(|e| e.to_string())?;
    Ok(gateway.server_status().await)
}

#[tauri::command]
pub async fn mcp_refresh_tools(gateway: State<'_, GatewayHandle>) -> Result<Value, String> {
    let reports = gateway.refresh_tools().await;
    let list: Vec<Value> = reports
        .iter()
        .map(|r| {
            serde_json::json!({
                "server": r.server,
                "ok": r.ok,
                "toolCount": r.tool_count,
                "error": r.error,
            })
        })
        .collect();
    Ok(serde_json::json!({ "refreshed": list }))
}

/// Operator kill switch: immediately disable routing to a backend.
#[tauri::command]
pub async fn mcp_kill_server(gateway: State<'_, GatewayHandle>, name: String) -> Result<Value, String> {
    gateway.kill_server(&name).await.map_err(|e| e.to_string())?;
    Ok(gateway.server_status().await)
}

/// Revive a killed / circuit-open backend and reset its error budget.
#[tauri::command]
pub async fn mcp_revive_server(gateway: State<'_, GatewayHandle>, name: String) -> Result<Value, String> {
    gateway.revive_server(&name).await.map_err(|e| e.to_string())?;
    Ok(gateway.server_status().await)
}

/// 数据目录下 `mcp-servers.json` 的绝对路径（三种数据目录模式统一解析）。
fn mcp_config_path(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    let default_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let data_dir = crate::data_dir::resolve_data_dir_with_mode(default_dir).data_dir;
    Ok(data_dir.join("mcp-servers.json"))
}

/// 读取当前 MCP 注册表配置（供界面表单回显），并附带配置文件路径。
#[tauri::command]
pub fn mcp_get_config(app: AppHandle) -> Result<Value, String> {
    let path = mcp_config_path(&app)?;
    let config = app_mcp::load_config(&path).map_err(|e| format!("读取 MCP 配置失败：{e}"))?;
    Ok(serde_json::json!({
        "path": path.to_string_lossy(),
        "config": config,
    }))
}

/// 覆盖写入 MCP 注册表配置，并让网关立即重新加载生效。
#[tauri::command]
pub async fn mcp_save_config(
    app: AppHandle,
    gateway: State<'_, GatewayHandle>,
    config: Value,
) -> Result<Value, String> {
    let parsed: McpRegistryConfig = serde_json::from_value(config).map_err(|e| format!("配置格式不正确：{e}"))?;

    let path = mcp_config_path(&app)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建数据目录失败：{e}"))?;
    }
    let text = serde_json::to_string_pretty(&parsed).map_err(|e| e.to_string())?;
    std::fs::write(&path, text).map_err(|e| format!("写入 MCP 配置失败：{e}"))?;

    gateway
        .reload()
        .await
        .map_err(|e| format!("重新加载 MCP 配置失败：{e}"))?;
    Ok(gateway.server_status().await)
}
