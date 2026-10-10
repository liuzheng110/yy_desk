//! 大栈 tokio runtime。
//!
//! 移植自 dbx 的 `dbx_core::scheduled_backup::worker_runtime`：
//! 元数据/命令链会嵌套非常大的异步 future（单帧可达 60-150 KiB），
//! 放不进 tokio 默认的 2 MiB worker 栈，会导致
//! `fatal runtime error: stack overflow`。所有驱动这些 future 的
//! runtime（桌面、后台 worker、Web server）都必须使用同样的宽栈。

pub const WORKER_STACK_SIZE: usize = 16 * 1024 * 1024;

/// 构建被所有进程复用的多线程 runtime，栈足够容纳嵌套 future。
pub fn worker_runtime() -> std::io::Result<tokio::runtime::Runtime> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(WORKER_STACK_SIZE)
        .build()
}
