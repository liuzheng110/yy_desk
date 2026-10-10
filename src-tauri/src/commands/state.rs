//! 全局应用状态。dbx 的 `AppState` 承载大量数据库运行态，
//! 框架只保留存储句柄；业务状态请按需扩展此结构。

use app_core::storage::Storage;
use std::sync::Arc;

pub struct AppState {
    pub storage: Arc<Storage>,
}
