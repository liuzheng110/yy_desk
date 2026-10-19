//! 启动管理命令（launcher）。
//!
//! 插件化设计：`data_dir/launch-items.json` 定义启动项（exe / .bat / .cmd / .ps1 / .py），
//! 往配置文件加一个条目即出现在"启动管理"页，随插随用，无需改代码。
//!
//! 交互语义（按钮一键切换）：
//! - 目标未运行 → 启动进程（返回 `started`）
//! - 目标已运行 → 把窗口呼出到最前台（返回 `focused`）
//!
//! 状态检测按 `process_name` 匹配系统进程（默认取 command 文件名去扩展名）。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use serde::{Deserialize, Serialize};
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, RefreshKind, System, UpdateKind};
use tauri::Manager;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchItem {
    pub id: String,
    pub name: String,
    /// 可执行文件或脚本（.exe / .bat / .cmd / .ps1 / .py）
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub workdir: Option<String>,
    /// 进程名（不含扩展名），留空则取 command 文件名
    #[serde(default)]
    pub process_name: Option<String>,
    /// 呼出前台时可选的窗口标题匹配（留空则按进程主窗口）
    #[serde(default)]
    pub window_title: Option<String>,
    /// 可选：服务端口（web 类项目用端口监听判断运行状态，优先于 process_name）
    #[serde(default)]
    pub port: Option<u16>,
    /// 可选：运行中点击"呼出"时打开的 URL（web 类项目打开界面）
    #[serde(default)]
    pub url: Option<String>,
    /// 可选：停止命令（脚本类项目用自带的停止逻辑，如 start-web.cmd -Stop；留空则按进程名结束进程树）
    #[serde(default)]
    pub stop_command: Option<String>,
    #[serde(default)]
    pub stop_args: Vec<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchStatus {
    pub id: String,
    pub running: bool,
}

fn default_true() -> bool {
    true
}

/// 配置文件：data_dir/launch-items.json（与 app.db / mcp-servers.json 同目录）。
fn config_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let default_data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let resolution = crate::data_dir::resolve_data_dir_with_mode(default_data_dir);
    Ok(resolution.data_dir.join("launch-items.json"))
}

/// 新安装从空列表开始；个人启动项只保存在数据目录，不随源码分发。
fn default_items() -> Vec<LaunchItem> {
    Vec::new()
}

/// 列出全部启动项（配置不存在时自动创建默认配置）。
#[tauri::command]
pub fn list_launch_items(app: tauri::AppHandle) -> Result<Vec<LaunchItem>, String> {
    let path = config_path(&app)?;
    let items = if path.exists() {
        let text = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
        serde_json::from_str::<Vec<LaunchItem>>(&text).map_err(|e| format!("launch-items.json 解析失败: {e}"))?
    } else {
        let items = default_items();
        let json = serde_json::to_string_pretty(&items).map_err(|e| e.to_string())?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::write(&path, json).map_err(|e| e.to_string())?;
        items
    };
    Ok(items)
}

/// 启动或呼出：未运行 → 启动进程；已运行 → 呼出到最前台（有 url 则打开页面）。
/// 返回 "started" 或 "focused"。
#[tauri::command]
pub fn launch_item(app: tauri::AppHandle, id: String) -> Result<String, String> {
    let items = list_launch_items(app)?;
    let item = items
        .iter()
        .find(|i| i.id == id)
        .ok_or_else(|| format!("启动项不存在: {id}"))?;
    if !item.enabled {
        return Err("启动项已禁用".into());
    }
    if running(item) {
        if let Some(url) = &item.url {
            open_url(url).map_err(|e| format!("打开页面失败: {e}"))?;
        } else {
            focus(item).map_err(|e| format!("呼出失败: {e}"))?;
        }
        return Ok("focused".into());
    }
    spawn_item(item).map_err(|e| format!("启动失败: {e}"))?;
    Ok("started".into())
}

/// 停止：已配置 stop_command 则调用停止脚本（web 服务类，优雅停服务）；
/// 否则按进程名结束整个进程树。返回 "stopped"。
#[tauri::command]
pub fn stop_launch_item(app: tauri::AppHandle, id: String) -> Result<String, String> {
    let items = list_launch_items(app)?;
    let item = items
        .iter()
        .find(|i| i.id == id)
        .ok_or_else(|| format!("启动项不存在: {id}"))?;
    if !running(item) {
        return Ok("stopped".into()); // 本来就没在运行
    }
    if let Some(stop_cmd) = &item.stop_command {
        let lower = stop_cmd.to_ascii_lowercase();
        let mut command = if lower.ends_with(".bat") || lower.ends_with(".cmd") {
            let mut c = std::process::Command::new("cmd");
            c.arg("/C").arg("call").arg(stop_cmd);
            c
        } else if lower.ends_with(".ps1") {
            let mut c = std::process::Command::new("powershell");
            c.args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
                .arg(stop_cmd);
            c
        } else {
            std::process::Command::new(stop_cmd)
        };
        if !item.stop_args.is_empty() {
            command.args(&item.stop_args);
        }
        if let Some(wd) = &item.workdir {
            command.current_dir(wd);
        }
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        command.spawn().map_err(|e| format!("停止脚本调用失败: {e}"))?;
        return Ok("stopped".into());
    }
    // 默认：按进程名结束进程树（taskkill /T 连带子进程）
    let name = process_name_of(item);
    let pid = find_pid(&name).ok_or_else(|| format!("未找到进程 {name}，可能已停止"))?;
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        let mut c = std::process::Command::new("taskkill");
        c.args(["/PID", &pid.to_string(), "/T", "/F"]);
        c.creation_flags(0x08000000);
        c.spawn().map_err(|e| format!("结束进程失败: {e}"))?;
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = pid;
    }
    Ok("stopped".into())
}

/// 批量查询运行状态。
#[tauri::command]
pub fn list_launch_statuses(app: tauri::AppHandle) -> Result<Vec<LaunchStatus>, String> {
    let items = list_launch_items(app)?;
    Ok(items
        .iter()
        .map(|i| LaunchStatus {
            id: i.id.clone(),
            running: running(i),
        })
        .collect())
}

/// 目标进程名（process_name 或 command 文件名去扩展名）。
fn process_name_of(item: &LaunchItem) -> String {
    item.process_name.clone().unwrap_or_else(|| {
        Path::new(&item.command)
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default()
    })
}

/// 目标是否在运行：配置了 port 则探测端口监听（web 服务类），否则按进程名匹配。
fn running(item: &LaunchItem) -> bool {
    if let Some(port) = item.port {
        return std::net::TcpStream::connect(("127.0.0.1", port)).is_ok();
    }
    find_pid(&process_name_of(item)).is_some()
}

/// 按进程名找 pid（大小写不敏感，兼容带/不带 .exe 扩展名）。
fn find_pid(name: &str) -> Option<u32> {
    let mut system = System::new_with_specifics(
        RefreshKind::new().with_processes(ProcessRefreshKind::new().with_cmd(UpdateKind::OnlyIfNotSet)),
    );
    system.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::new().with_cmd(UpdateKind::OnlyIfNotSet),
    );
    for (pid, process) in system.processes() {
        let raw = process.name().to_string_lossy();
        // Windows 下 sysinfo 的进程名通常带扩展名（如 "codex-cost-ledger.exe"），
        // 配置里可能写带或不带扩展名，都兼容。
        let no_ext = raw.rsplit_once('.').map(|(s, _)| s).unwrap_or(&raw);
        if no_ext.eq_ignore_ascii_case(name) || raw.eq_ignore_ascii_case(name) {
            return Some(pid.as_u32());
        }
    }
    None
}

/// 启动进程（按扩展名分派；spawn 后分离，进程独立运行；Windows 隐藏控制台窗口）。
fn spawn_item(item: &LaunchItem) -> std::io::Result<()> {
    let cmd = &item.command;
    let lower = cmd.to_ascii_lowercase();
    let mut command = if lower.ends_with(".bat") || lower.ends_with(".cmd") {
        let mut c = std::process::Command::new("cmd");
        c.arg("/C").arg("call").arg(cmd);
        c
    } else if lower.ends_with(".ps1") {
        let mut c = std::process::Command::new("powershell");
        c.args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"]).arg(cmd);
        c
    } else if lower.ends_with(".py") {
        let mut c = std::process::Command::new("python");
        c.arg(cmd);
        c
    } else {
        std::process::Command::new(cmd)
    };
    if !item.args.is_empty() {
        command.args(&item.args);
    }
    if let Some(wd) = &item.workdir {
        command.current_dir(wd);
    }
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        // 0x08000000 = CREATE_NO_WINDOW：脚本/控制台程序不弹黑窗
        command.creation_flags(0x08000000);
    }
    command.spawn().map(|_| ())
}

/// 打开 URL（Windows 用默认浏览器；其他平台用系统 opener）。
fn open_url(url: &str) -> std::io::Result<()> {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        let mut c = std::process::Command::new("cmd");
        c.args(["/C", "start", "", url]);
        c.creation_flags(0x08000000);
        c.spawn().map(|_| ())
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = std::process::Command::new("xdg-open").arg(url).spawn();
        Ok(())
    }
}

/// 呼出到最前台（Windows：找进程主窗口 → 还原 → 置顶激活）。
#[cfg(target_os = "windows")]
fn focus(item: &LaunchItem) -> std::io::Result<()> {
    use windows_sys::core::BOOL;
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::WindowsAndMessaging::*;

    static FOUND: AtomicUsize = AtomicUsize::new(0);

    unsafe extern "system" fn enum_proc(hwnd: HWND, lparam: isize) -> BOOL {
        let target_pid = lparam as u32;
        let mut pid: u32 = 0;
        GetWindowThreadProcessId(hwnd, &mut pid);
        if pid == target_pid && IsWindowVisible(hwnd) != 0 {
            FOUND.store(hwnd as usize, Ordering::SeqCst);
            return 0; // 停止枚举
        }
        1
    }

    let name = process_name_of(item);
    let pid = find_pid(&name).ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "process not found"))?;

    FOUND.store(0, Ordering::SeqCst);
    unsafe {
        EnumWindows(Some(enum_proc), pid as isize);
        let hwnd = FOUND.load(Ordering::SeqCst) as HWND;
        if hwnd.is_null() {
            // 进程在跑但没有可见顶层窗口（如脚本/后台任务）：视为已呼出
            return Ok(());
        }
        ShowWindow(hwnd, SW_RESTORE);
        SetForegroundWindow(hwnd);
    }
    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn focus(_item: &LaunchItem) -> std::io::Result<()> {
    Ok(())
}
