//! 系统字体枚举。
//! 移植自 dbx 的 `dbx_core::connection::jdbc::list_system_fonts`（font-kit 实现）。

/// 返回去重排序后的系统字体族名称。
pub fn list_system_fonts() -> Vec<String> {
    let source = font_kit::source::SystemSource::new();
    match source.all_families() {
        Ok(families) => {
            use std::collections::BTreeSet;
            families
                .into_iter()
                .map(|family| family.trim().to_string())
                .filter(|family| !family.is_empty())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect()
        }
        Err(_) => vec![],
    }
}
