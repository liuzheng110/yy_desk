//! 小工具：SQLite 文件探测、路径展开。

/// 判断文件是否带 SQLite 头部魔数（"SQLite format 3\0"）。
/// 移植自 dbx 的 `dbx_core::db::sqlite::path_has_sqlite_header`。
pub fn path_has_sqlite_header(path: &std::path::Path) -> bool {
    const SQLITE_MAGIC: &[u8] = b"SQLite format 3\0";
    use std::io::Read;
    let mut file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(_) => return false,
    };
    let mut buf = [0u8; 16];
    if file.read_exact(&mut buf).is_err() {
        return false;
    }
    &buf == SQLITE_MAGIC
}
