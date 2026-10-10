//! 命令行/深链接携带的外部文件打开。
//!
//! 替代 dbx 的 external_sql / external_db（那两套按数据库类型解析并
//! 执行 SQL，属于业务）；框架只负责「把路径安全地交给前端」。

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

#[derive(Default)]
pub struct ExternalFileOpenState {
    pending: Mutex<Vec<PathBuf>>,
}

impl ExternalFileOpenState {
    pub fn push(&self, paths: Vec<PathBuf>) {
        if let Ok(mut pending) = self.pending.lock() {
            pending.extend(paths);
        }
    }

    pub fn take_all(&self) -> Vec<PathBuf> {
        self.pending.lock().map(|mut pending| std::mem::take(&mut *pending)).unwrap_or_default()
    }
}

pub fn new_external_file_open_state() -> Arc<ExternalFileOpenState> {
    Arc::new(ExternalFileOpenState::default())
}

/// 从启动参数中提取文件路径（跳过以 `-` 开头的选项与深链接 scheme）。
pub fn file_paths_from_args(args: Vec<String>, cwd: &Path) -> Vec<PathBuf> {
    args.iter()
        .filter(|arg| !arg.starts_with('-') && !arg.contains("://"))
        .map(|arg| {
            let path = PathBuf::from(arg);
            if path.is_absolute() {
                path
            } else {
                cwd.join(path)
            }
        })
        .filter(|path| path.is_file())
        .collect()
}

/// 前端取走待打开文件（单实例交接场景）。
#[tauri::command]
pub fn take_pending_files(state: tauri::State<'_, Arc<ExternalFileOpenState>>) -> Vec<String> {
    state.take_all().into_iter().map(|path| path.to_string_lossy().into_owned()).collect()
}
