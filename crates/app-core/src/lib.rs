//! app-core：MyApp 桌面框架的核心库。
//!
//! 从 dbx 的 dbx-core 裁剪而来，只保留框架级基础设施：
//! - `storage`：SQLite 设置的持久化存储（桌面设置/外观/通用 JSON）
//! - `update`：自动更新检查与发布信息（多源 + 系统代理）
//! - `changelog`：更新日志获取
//! - `worker_runtime`：大栈 tokio runtime（防深嵌套 future 栈溢出）
//! - `fonts` / `fs`：系统字体枚举、文件探测等小工具
//!
//! 数据库驱动、SQL 引擎、AI、MCP 等业务模块已全部移除，
//! 你的业务逻辑请放在 `src-tauri/src/commands/business/` 或独立 crate 中。

pub mod changelog;
pub mod fonts;
pub mod fs;
pub mod storage;
pub mod update;
pub mod worker_runtime;

pub use app_platform::download::{race_download_urls, GITHUB_RELEASE_DOWNLOAD_PREFIX};
pub use app_platform::{path_utils, process};
