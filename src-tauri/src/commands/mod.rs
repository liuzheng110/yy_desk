//! IPC 命令注册表。
//!
//! 框架级命令：设置、窗口、更新、深链接、系统信息。
//! 你的业务命令请新建 `business/` 子目录并在 `mod.rs` 中声明。

pub mod app_info;
pub mod app_settings;
pub mod autostart;
pub mod deep_link;
pub mod diagnostics;
pub mod external_files;
pub mod fs_open;
pub mod keychain;
pub mod launch_args;
pub mod mcp;
pub mod state;
pub mod support_info;
pub mod system_fonts;
pub mod update;
pub mod update_cache;
pub mod update_portable;
pub mod window_controls;
