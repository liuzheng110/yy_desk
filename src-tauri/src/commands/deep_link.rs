//! 深链接处理。
//!
//! 裁剪自 dbx 的 deep_link.rs：去掉数据库连接/AI 配置/插件安装等
//! 业务链接路由，保留通用 `myapp://` 链接的解析与单实例交接。
//!
//! TODO(可选)：如需业务深链接（如 `myapp://open/xxx`），在
//! `route_links` 中按 scheme 分发并 emit 给前端。

use std::sync::Mutex;

use app_platform::identity::APP_SCHEME;

#[derive(Default)]
pub struct DeepLinkOpenState {
    last_routed: Mutex<Option<Vec<String>>>,
}

impl DeepLinkOpenState {
    pub fn new() -> Self {
        Self {
            last_routed: Mutex::new(None),
        }
    }

    /// 返回 true 表示链接是新批次（应 emit 给前端）。
    /// 单实例交接可能反复触发，用最近一次路由去重。
    pub fn route_links(&self, links: Vec<String>) -> bool {
        let mut last = self.last_routed.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if last.as_ref() == Some(&links) {
            return false;
        }
        *last = Some(links);
        true
    }
}

/// 判断参数是否为应用深链接。
pub fn is_app_open_deep_link(arg: &str) -> bool {
    arg.starts_with(APP_SCHEME)
}

/// 从启动参数中提取深链接。
pub fn deep_links_from_args(args: Vec<String>) -> Vec<String> {
    args.into_iter().filter(|arg| is_app_open_deep_link(arg)).collect()
}

/// 解析 `myapp://<path>?<query>` 为结构化链接。
#[derive(Debug, Clone, serde::Serialize)]
pub struct DeepLink {
    pub scheme: String,
    pub path: String,
    pub query: std::collections::HashMap<String, String>,
}

#[tauri::command]
pub fn parse_deep_link(link: String) -> Result<DeepLink, String> {
    let rest = link
        .strip_prefix(APP_SCHEME)
        .ok_or_else(|| format!("unsupported deep link scheme: {link}"))?;
    let (path_part, query_part) = rest.split_once('?').unwrap_or((rest, ""));
    let path = percent_encoding::percent_decode_str(path_part)
        .decode_utf8_lossy()
        .to_string();
    let mut query = std::collections::HashMap::new();
    for pair in query_part.split('&').filter(|pair| !pair.is_empty()) {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        let key = percent_encoding::percent_decode_str(key)
            .decode_utf8_lossy()
            .to_string();
        let value = percent_encoding::percent_decode_str(value)
            .decode_utf8_lossy()
            .to_string();
        query.insert(key, value);
    }
    Ok(DeepLink {
        scheme: APP_SCHEME.trim_end_matches("://").to_string(),
        path,
        query,
    })
}
