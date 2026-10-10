//! SQLite 持久化存储。
//!
//! 从 dbx 的 `dbx_core::persistence::storage` 裁剪而来：
//! 保留「单一 SQLite 数据库承载应用设置」的模式与公开 API 形态，
//! 移除了数据库连接、AI、MCP、SQL 历史等业务表。
//!
//! 结构：一张 `settings` 表（key-value，值存 JSON），
//! 一张 `data_migrations` 表记录已应用的迁移（为将来扩展预留）。

use rusqlite::{Connection, OptionalExtension};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

const SETTINGS_TABLE: &str = "CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT NOT NULL)";
const MIGRATIONS_TABLE: &str =
    "CREATE TABLE IF NOT EXISTS data_migrations (id TEXT PRIMARY KEY, applied_at TEXT NOT NULL)";

const DESKTOP_SETTINGS_KEY: &str = "desktop_settings";
const APP_APPEARANCE_KEY: &str = "app_appearance";

/// 数据迁移状态。dbx 用它驱动「迁移完成前拒绝业务命令」的门槛，
/// 新框架首次启动即建好 schema，因此默认 ready；将来接入自己的
/// 数据迁移时，把 `inspect_data_migration` 改为真实检查即可。
#[derive(Debug, Clone)]
pub struct MigrationStatus {
    ready: bool,
}

impl MigrationStatus {
    pub fn is_ready(&self) -> bool {
        self.ready
    }
}

/// 桌面设置（裁剪自 dbx 的 DesktopSettings，去掉数据库/驱动相关字段）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopSettings {
    #[serde(default = "default_true")]
    pub show_tray_icon: bool,
    #[serde(default)]
    pub icon_theme: DesktopIconTheme,
    #[serde(default)]
    pub quit_on_close: bool,
    #[serde(default)]
    pub close_action_prompted: bool,
    #[serde(default)]
    pub debug_logging_enabled: bool,
}

fn default_true() -> bool {
    true
}

impl Default for DesktopSettings {
    fn default() -> Self {
        Self {
            show_tray_icon: true,
            icon_theme: DesktopIconTheme::Default,
            quit_on_close: false,
            close_action_prompted: false,
            debug_logging_enabled: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DesktopIconTheme {
    #[default]
    Default,
    Black,
}

/// 外观设置：渲染端在模块初始化前就需要读取，因此与桌面设置分开存储。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppAppearanceSettings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locale: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme_mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme_palette: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_ui_colors: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_ui_colors_dark: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub corner_style: Option<String>,
}

/// 部分外观更新。多个控件快速切换时在同一事务内合并，避免互相覆盖。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppAppearanceSettingsPatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locale: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme_mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme_palette: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_ui_colors: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_ui_colors_dark: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub corner_style: Option<String>,
}

/// 数据目录回退导入的结果（便携版 → 默认目录 的数据迁移）。
#[derive(Debug, Clone)]
pub enum DataDbImportResult {
    NoImport,
    Imported { from: PathBuf, to: PathBuf },
}

/// 若备选数据目录中存在应用数据库而目标目录没有，则复制过来。
/// 移植自 dbx 的 `maybe_import_user_data_db`（简化版，不处理加密/密钥迁移）。
pub fn maybe_import_user_data_db(
    data_dir: &Path,
    alternative_data_dir: Option<&Path>,
) -> Result<DataDbImportResult, String> {
    let Some(alt) = alternative_data_dir else {
        return Ok(DataDbImportResult::NoImport);
    };
    let target_db = data_dir.join("app.db");
    let alt_db = alt.join("app.db");
    if target_db.exists() || !alt_db.exists() {
        return Ok(DataDbImportResult::NoImport);
    }
    std::fs::create_dir_all(data_dir).map_err(|e| format!("Failed to create data dir: {e}"))?;
    std::fs::copy(&alt_db, &target_db).map_err(|e| format!("Failed to import user data db: {e}"))?;
    Ok(DataDbImportResult::Imported {
        from: alt_db,
        to: target_db,
    })
}

/// 应用设置存储。内部是单连接 + Mutex，简单可靠；
/// 未来若需要并发读写可换 r2d2/deadpool 连接池。
pub struct Storage {
    db: Arc<Mutex<Connection>>,
}

impl Storage {
    /// 打开（不存在则创建）数据库并建好基础 schema。
    /// 名字保留 dbx 的 `open_unmigrated`：迁移在打开时即视为完成。
    pub async fn open_unmigrated(path: &Path) -> anyhow::Result<Storage> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.execute_batch(&format!("{SETTINGS_TABLE}; {MIGRATIONS_TABLE};"))?;
        Ok(Storage {
            db: Arc::new(Mutex::new(conn)),
        })
    }

    /// 数据迁移状态。新框架首次打开即 ready。
    pub async fn inspect_data_migration(&self) -> Result<MigrationStatus, String> {
        Ok(MigrationStatus { ready: true })
    }

    pub async fn load_desktop_settings(&self) -> Result<DesktopSettings, String> {
        Ok(self
            .get_json(DESKTOP_SETTINGS_KEY)
            .await?
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default())
    }

    pub async fn save_desktop_settings(&self, settings: &DesktopSettings) -> Result<(), String> {
        self.set_json(
            DESKTOP_SETTINGS_KEY,
            &serde_json::to_value(settings).map_err(|e| e.to_string())?,
        )
        .await
    }

    pub async fn load_app_appearance_settings(&self) -> Result<AppAppearanceSettings, String> {
        Ok(self
            .get_json(APP_APPEARANCE_KEY)
            .await?
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default())
    }

    /// 应用部分外观更新：与现有值合并后整体写回。
    pub async fn update_app_appearance_settings(&self, patch: &AppAppearanceSettingsPatch) -> Result<(), String> {
        let mut current = self.load_app_appearance_settings().await?;
        if patch.locale.is_some() {
            current.locale = patch.locale.clone();
        }
        if patch.theme_mode.is_some() {
            current.theme_mode = patch.theme_mode.clone();
        }
        if patch.theme_palette.is_some() {
            current.theme_palette = patch.theme_palette.clone();
        }
        if patch.custom_ui_colors.is_some() {
            current.custom_ui_colors = patch.custom_ui_colors.clone();
        }
        if patch.custom_ui_colors_dark.is_some() {
            current.custom_ui_colors_dark = patch.custom_ui_colors_dark.clone();
        }
        if patch.corner_style.is_some() {
            current.corner_style = patch.corner_style.clone();
        }
        self.set_json(
            APP_APPEARANCE_KEY,
            &serde_json::to_value(&current).map_err(|e| e.to_string())?,
        )
        .await
    }

    /// 多标签页/窗口状态存储（如 open_tabs、detached_tabs）。
    pub async fn load_open_tabs_state_with_key(&self, key: &str) -> Result<Option<Value>, String> {
        self.get_json(key).await
    }

    pub async fn save_open_tabs_state_with_key(&self, key: &str, value: &Value) -> Result<(), String> {
        self.set_json(key, value).await
    }

    /// 通用 JSON 读取（框架业务也可用：把任意设置存成 JSON）。
    pub async fn get_json(&self, key: &str) -> Result<Option<Value>, String> {
        let conn = self.db.lock().map_err(|e| e.to_string())?;
        let row: Option<String> = conn
            .query_row("SELECT value FROM settings WHERE key = ?1", [key], |row| row.get(0))
            .optional()
            .map_err(|e| e.to_string())?;
        match row {
            Some(text) => serde_json::from_str(&text).map(Some).map_err(|e| e.to_string()),
            None => Ok(None),
        }
    }

    /// 通用 JSON 写入（UPSERT）。
    pub async fn set_json(&self, key: &str, value: &Value) -> Result<(), String> {
        let text = serde_json::to_string(value).map_err(|e| e.to_string())?;
        let conn = self.db.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [key, &text],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// 泛型读写便捷封装。
    pub async fn get_setting<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>, String> {
        Ok(self.get_json(key).await?.and_then(|v| serde_json::from_value(v).ok()))
    }

    pub async fn set_setting<T: Serialize>(&self, key: &str, value: &T) -> Result<(), String> {
        let json = serde_json::to_value(value).map_err(|e| e.to_string())?;
        self.set_json(key, &json).await
    }
}
