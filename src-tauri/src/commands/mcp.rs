//! MCP 网关管理命令。
//!
//! - `mcp_status`：网关端口、服务器列表、各自缓存的工具数（供界面/CLI 展示）
//! - `mcp_reload_config`：重新加载 `data/mcp-servers.json`
//! - `mcp_refresh_tools`：逐个连接外部 MCP 扫描工具清单，写入持久化缓存

use serde_json::Value;
use tauri::State;

use app_mcp::gateway::GatewayHandle;

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
pub async fn mcp_kill_server(
    gateway: State<'_, GatewayHandle>,
    name: String,
) -> Result<Value, String> {
    gateway.kill_server(&name).await.map_err(|e| e.to_string())?;
    Ok(gateway.server_status().await)
}

/// Revive a killed / circuit-open backend and reset its error budget.
#[tauri::command]
pub async fn mcp_revive_server(
    gateway: State<'_, GatewayHandle>,
    name: String,
) -> Result<Value, String> {
    gateway.revive_server(&name).await.map_err(|e| e.to_string())?;
    Ok(gateway.server_status().await)
}
