//! Local endpoint inventory. Stopping is bound to a PID + creation time, never a name/port alone.
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Endpoint {
    protocol: String,
    address: String,
    port: u16,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PortProcess {
    pid: u32,
    name: String,
    executable: String,
    command_line: String,
    parent_pid: u32,
    // String preserves all 64 FILETIME bits through JavaScript.
    started_at: String,
    protected_reason: String,
    endpoints: Vec<Endpoint>,
    #[serde(default)]
    project_path: Option<String>,
    #[serde(default)]
    development: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StopTarget {
    pid: u32,
    started_at: String,
}

#[derive(Debug, Serialize)]
pub struct StopResult {
    pid: u32,
    stopped: bool,
    message: String,
}

// Only tokenize for path hints; never execute or reconstruct a shell command.
fn command_paths(command: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut token = String::new();
    let mut quote = None;
    for ch in command.chars() {
        if matches!(ch, '\'' | '"') && (quote.is_none() || quote == Some(ch)) {
            quote = if quote.is_some() { None } else { Some(ch) };
        } else if ch.is_whitespace() && quote.is_none() {
            if !token.is_empty() {
                result.push(std::mem::take(&mut token));
            }
        } else {
            token.push(ch);
        }
    }
    if !token.is_empty() {
        result.push(token);
    }
    result
}

fn project_from_path(candidate: &Path) -> Option<PathBuf> {
    if !candidate.is_absolute() {
        return None;
    }
    #[cfg(windows)]
    if !matches!(candidate.components().next(), Some(std::path::Component::Prefix(prefix))
        if matches!(prefix.kind(), std::path::Prefix::Disk(_) | std::path::Prefix::VerbatimDisk(_)))
    {
        // Do not probe remote UNC shares while inspecting local command lines.
        return None;
    }
    // A dependency's own package.json is not the user's project.
    let components: Vec<_> = candidate.components().collect();
    let dependency = components.iter().position(|c| {
        let name = c.as_os_str().to_string_lossy();
        name.eq_ignore_ascii_case("node_modules") || name.eq_ignore_ascii_case(".venv")
    });
    let path = dependency
        .map(|index| components[..index].iter().collect::<PathBuf>())
        .unwrap_or_else(|| candidate.to_path_buf());
    let start = if path.is_dir() { path.as_path() } else { path.parent()? };
    start.ancestors().take(12).find_map(|dir| {
        [
            "package.json",
            "Cargo.toml",
            "go.mod",
            "pyproject.toml",
            "requirements.txt",
            ".git",
            "pom.xml",
            "build.gradle",
            "build.gradle.kts",
            "project.config.json",
        ]
        .iter()
        .any(|marker| dir.join(marker).exists())
        .then(|| dir.to_path_buf())
    })
}

fn enrich(process: &mut PortProcess) {
    process.project_path = command_paths(&process.command_line)
        .iter()
        .chain(std::iter::once(&process.executable))
        .find_map(|path| project_from_path(Path::new(path)))
        .map(|path| path.to_string_lossy().into_owned());
    let name = process.name.to_ascii_lowercase();
    process.development = process.project_path.is_some()
        || [
            "node", "bun", "deno", "python", "python3", "pythonw", "java", "javaw", "dotnet", "php", "ruby", "uvicorn",
        ]
        .iter()
        .any(|runtime| name == *runtime || name == format!("{runtime}.exe"));
}

#[cfg(windows)]
async fn scan() -> Result<Vec<PortProcess>, String> {
    use std::time::Duration;
    let shell = PathBuf::from(std::env::var_os("SystemRoot").ok_or("找不到 Windows 目录")?)
        .join("System32/WindowsPowerShell/v1.0/powershell.exe");
    let mut command = app_platform::process::new_tokio_command(shell);
    command
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            include_str!("port_scan.ps1"),
        ])
        .env("MYAPP_PORT_MONITOR_PID", std::process::id().to_string())
        .kill_on_drop(true);
    let output = tokio::time::timeout(Duration::from_secs(30), command.output())
        .await
        .map_err(|_| "端口扫描超时，请重试".to_string())?
        .map_err(|e| format!("无法启动端口扫描: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "端口扫描失败: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let mut rows: Vec<PortProcess> =
        serde_json::from_slice(&output.stdout).map_err(|e| format!("无法解析端口扫描结果: {e}"))?;
    for row in &mut rows {
        enrich(row);
    }
    rows.sort_by_key(|row| (row.endpoints.iter().map(|e| e.port).min().unwrap_or(0), row.pid));
    Ok(rows)
}

#[cfg(not(windows))]
async fn scan() -> Result<Vec<PortProcess>, String> {
    Err("端口监测目前支持 Windows 桌面端".into())
}

#[tauri::command]
pub async fn scan_local_ports() -> Result<Vec<PortProcess>, String> {
    scan().await
}

fn validate_target<'a>(target: &StopTarget, rows: &'a [PortProcess]) -> Result<&'a PortProcess, String> {
    let process = rows
        .iter()
        .find(|row| row.pid == target.pid)
        .ok_or("该进程已退出或不再占用端口，请刷新")?;
    if target.started_at.is_empty() || process.started_at != target.started_at {
        return Err("进程身份已变化，请刷新后重新选择".into());
    }
    if !process.protected_reason.is_empty() {
        return Err(format!("此进程不可停止: {}", process.protected_reason));
    }
    Ok(process)
}

#[cfg(windows)]
fn terminate(target: &StopTarget) -> Result<(), String> {
    use windows_sys::Win32::{
        Foundation::{CloseHandle, FILETIME, WAIT_OBJECT_0},
        System::Threading::{
            GetProcessTimes, OpenProcess, TerminateProcess, WaitForSingleObject, PROCESS_QUERY_LIMITED_INFORMATION,
            PROCESS_SYNCHRONIZE, PROCESS_TERMINATE,
        },
    };
    let expected = target.started_at.parse::<u64>().map_err(|_| "无效的进程启动时间")?;
    // Retain this handle through identity verification and termination: PID reuse cannot retarget it.
    unsafe {
        let handle = OpenProcess(
            PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_TERMINATE | PROCESS_SYNCHRONIZE,
            0,
            target.pid,
        );
        if handle.is_null() {
            return Err(format!(
                "无法停止进程（可能已退出或权限不足）: {}",
                std::io::Error::last_os_error()
            ));
        }
        let result = (|| {
            let mut created: FILETIME = std::mem::zeroed();
            let mut exited: FILETIME = std::mem::zeroed();
            let mut kernel: FILETIME = std::mem::zeroed();
            let mut user: FILETIME = std::mem::zeroed();
            if GetProcessTimes(handle, &mut created, &mut exited, &mut kernel, &mut user) == 0 {
                return Err(format!("无法验证进程身份: {}", std::io::Error::last_os_error()));
            }
            let actual = ((created.dwHighDateTime as u64) << 32) | created.dwLowDateTime as u64;
            if actual != expected {
                return Err("进程身份已变化，未执行停止".into());
            }
            if TerminateProcess(handle, 1) == 0 {
                return Err(format!("停止失败: {}", std::io::Error::last_os_error()));
            }
            if WaitForSingleObject(handle, 3000) != WAIT_OBJECT_0 {
                return Err("已发出停止请求，但进程尚未退出，请刷新确认".into());
            }
            Ok(())
        })();
        CloseHandle(handle);
        result
    }
}

#[cfg(not(windows))]
fn terminate(_target: &StopTarget) -> Result<(), String> {
    Err("端口监测目前支持 Windows 桌面端".into())
}

#[tauri::command]
pub async fn stop_port_processes(targets: Vec<StopTarget>) -> Result<Vec<StopResult>, String> {
    if targets.is_empty() || targets.len() > 64 {
        return Err("请选择 1–64 个进程".into());
    }
    // Recheck system-service/ancestor protection on the backend, not just the UI.
    let rows = scan().await?;
    tauri::async_runtime::spawn_blocking(move || {
        let mut seen = std::collections::HashSet::new();
        targets
            .into_iter()
            .filter(|target| seen.insert(target.pid))
            .map(|target| {
                let result = validate_target(&target, &rows).and_then(|_| terminate(&target));
                StopResult {
                    pid: target.pid,
                    stopped: result.is_ok(),
                    message: result.err().unwrap_or_else(|| "进程已停止".into()),
                }
            })
            .collect()
    })
    .await
    .map_err(|e| format!("停止任务失败: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    // A dedicated child of the test binary owns ephemeral sockets. No existing service is stopped.
    #[test]
    #[cfg(windows)]
    fn port_fixture() {
        if std::env::var("MYAPP_PORT_TEST_FIXTURE").as_deref() != Ok("1") {
            return;
        }
        use std::io::Write;
        let tcp = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let udp = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        println!(
            "PORT_FIXTURE {} {}",
            tcp.local_addr().unwrap().port(),
            udp.local_addr().unwrap().port()
        );
        std::io::stdout().flush().unwrap();
        std::thread::sleep(std::time::Duration::from_secs(90));
    }

    #[tokio::test]
    #[cfg(windows)]
    async fn scans_and_stops_only_the_selected_process_incarnation() {
        use std::{process::Stdio, time::Duration};
        use tokio::io::{AsyncBufReadExt, BufReader};
        let own_listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let mut child = app_platform::process::new_tokio_command(std::env::current_exe().unwrap())
            .args(["--exact", "commands::ports::tests::port_fixture", "--nocapture"])
            .env("MYAPP_PORT_TEST_FIXTURE", "1")
            .stdout(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        let child_pid = child.id().unwrap();
        let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
        let ports = tokio::time::timeout(Duration::from_secs(15), async {
            while let Some(line) = lines.next_line().await.unwrap() {
                if let Some(ports) = line.strip_prefix("PORT_FIXTURE ") {
                    return ports
                        .split_whitespace()
                        .map(|port| port.parse::<u16>().unwrap())
                        .collect::<Vec<_>>();
                }
            }
            panic!("fixture exited before opening sockets");
        })
        .await
        .unwrap();
        let rows = scan().await.unwrap();
        let own_row = rows.iter().find(|row| row.pid == std::process::id()).unwrap();
        assert!(!own_row.protected_reason.is_empty());
        assert!(validate_target(
            &StopTarget {
                pid: own_row.pid,
                started_at: own_row.started_at.clone()
            },
            &rows
        )
        .is_err());
        let row = rows
            .iter()
            .find(|row| row.pid == child_pid)
            .expect("fixture not discovered");
        assert!(row.protected_reason.is_empty(), "{}", row.protected_reason);
        assert!(row.endpoints.iter().any(|e| e.protocol == "TCP" && e.port == ports[0]));
        assert!(row.endpoints.iter().any(|e| e.protocol == "UDP" && e.port == ports[1]));
        let stale = StopTarget {
            pid: child_pid,
            started_at: "1".into(),
        };
        assert!(validate_target(&stale, &rows).is_err());
        assert!(terminate(&stale).is_err());
        assert!(std::net::TcpStream::connect(("127.0.0.1", ports[0])).is_ok());
        let result = stop_port_processes(vec![StopTarget {
            pid: child_pid,
            started_at: row.started_at.clone(),
        }])
        .await
        .unwrap();
        assert!(result[0].stopped, "{}", result[0].message);
        tokio::time::timeout(Duration::from_secs(5), child.wait())
            .await
            .unwrap()
            .unwrap();
        assert!(std::net::TcpListener::bind(("127.0.0.1", ports[0])).is_ok());
        assert!(std::net::UdpSocket::bind(("127.0.0.1", ports[1])).is_ok());
        assert!(std::net::TcpStream::connect(own_listener.local_addr().unwrap()).is_ok());
    }

    fn process() -> PortProcess {
        serde_json::from_value(serde_json::json!({
            "pid": 123, "name": "node.exe", "executable": "node.exe", "command_line": "",
            "parent_pid": 1, "started_at": "123456789012345678", "protected_reason": "",
            "endpoints": [{"protocol": "TCP", "address": "::", "port": 8080}]
        }))
        .unwrap()
    }

    #[test]
    fn refuses_reused_pid_missing_identity_protected_and_exited_process() {
        let target = StopTarget {
            pid: 123,
            started_at: "123456789012345678".into(),
        };
        let mut row = process();
        assert!(validate_target(&target, &[row.clone()]).is_ok());
        row.started_at = "another-process".into();
        assert!(validate_target(&target, &[row]).is_err());
        let mut row = process();
        row.protected_reason = "system service".into();
        assert!(validate_target(&target, &[row]).is_err());
        assert!(validate_target(&target, &[]).is_err());
        assert!(validate_target(
            &StopTarget {
                pid: 123,
                started_at: String::new()
            },
            &[process()]
        )
        .is_err());
    }

    #[test]
    fn tokenizes_quoted_windows_paths_without_interpreting_shell_syntax() {
        assert_eq!(
            command_paths(r#""C:\Program Files\node.exe" "C:\My App\node_modules\vite\bin\vite.js" --port 8080"#),
            [
                r"C:\Program Files\node.exe",
                r"C:\My App\node_modules\vite\bin\vite.js",
                "--port",
                "8080"
            ]
        );
    }

    #[test]
    fn project_hint_ignores_dependency_manifests() {
        let dir = tempfile::tempdir().unwrap();
        let dependency = dir.path().join("node_modules/vite/bin");
        std::fs::create_dir_all(&dependency).unwrap();
        std::fs::write(dir.path().join("package.json"), "{}").unwrap();
        std::fs::write(dependency.parent().unwrap().join("package.json"), "{}").unwrap();
        assert_eq!(
            project_from_path(&dependency.join("vite.js")),
            Some(dir.path().to_path_buf())
        );
        assert_eq!(project_from_path(Path::new("relative/server.js")), None);
    }
}
