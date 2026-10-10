//! 业务命令脚手架。
//!
//! 新增业务命令的四步：
//! 1. 在本目录新建 `your_feature.rs`，用 `#[tauri::command]` 写命令函数；
//! 2. 在下面加一行 `pub mod your_feature;`（`commands/mod.rs` 里的 `pub mod business;` 已就绪）；
//! 3. 在 `lib.rs` 的 `generate_handler!` 里注册 `commands::business::your_feature::your_command`；
//! 4. 在 `apps/desktop/src/lib/backend.ts` 里加一层类型化封装，页面直接调用 backend.xxx()。
//!
//! 注意：命令若依赖 `tauri::State<T>`，必须在 `lib.rs` 的 Builder 上 `.manage(...)`
//! 注册对应状态，否则调用时会报 `state not managed for field ...`。

/// 示例：最小同步命令。
#[tauri::command]
pub fn business_ping(name: Option<String>) -> String {
    let who = name.unwrap_or_else(|| "world".to_string());
    format!("pong: {who}")
}

/// 示例：带参数校验的异步命令，返回结构化 JSON。
#[tauri::command]
pub async fn business_echo(payload: serde_json::Value) -> Result<serde_json::Value, String> {
    if payload.is_null() {
        return Err("payload 不能为空".to_string());
    }
    Ok(serde_json::json!({
        "echo": payload,
        "length": payload.to_string().len(),
    }))
}
