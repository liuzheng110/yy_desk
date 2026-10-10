//! Lazily start a configured desktop app, independently of short-lived MCP clients.
use std::{collections::BTreeMap, process::Stdio, sync::Arc, time::Duration};

use anyhow::{bail, Context};
use tokio::{
    net::TcpStream,
    process::{Child, Command},
    sync::Mutex,
    time::{sleep, timeout, Instant},
};

use crate::config::AppAutoStartConfig;

#[derive(Default)]
struct LaunchState {
    child: Option<Child>,
    last_launch: Option<Instant>,
}

#[derive(Clone, Default)]
pub struct AppLauncher {
    // The same desktop executable may back several MCP server entries.
    locks: Arc<Mutex<BTreeMap<String, Arc<Mutex<LaunchState>>>>>,
}

impl AppLauncher {
    pub async fn ensure_ready(&self, config: Option<&AppAutoStartConfig>) -> anyhow::Result<()> {
        let Some(config) = config.filter(|c| c.enabled) else {
            return Ok(());
        };
        if config.command.trim().is_empty() || config.port_file.trim().is_empty() {
            bail!("应用自动启动配置缺少 command 或 portFile");
        }
        if config.startup_timeout_ms == 0 || config.startup_timeout_ms > 120_000 {
            bail!("应用自动启动 startupTimeoutMs 必须在 1 到 120000 毫秒之间");
        }
        let budget = Duration::from_millis(config.startup_timeout_ms);
        timeout(budget, async {
            if app_ready(config).await {
                return Ok(());
            }
            let key = config.command.to_lowercase();
            let lock = self.locks.lock().await.entry(key).or_default().clone();
            let mut state = lock.lock().await;
            // Another concurrent request may already have brought the app up.
            if app_ready(config).await {
                return Ok(());
            }

            let running = match state.child.as_mut() {
                Some(child) => child.try_wait().context("检查自动启动的应用状态失败")?.is_none(),
                None => false,
            };
            // Also cover single-instance launchers that hand off to another process.
            let recently_started = state.last_launch.is_some_and(|t| t.elapsed() < budget);
            if !running && !recently_started {
                let mut command = Command::new(&config.command);
                command
                    .args(&config.args)
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .env_remove("ELECTRON_RUN_AS_NODE")
                    .kill_on_drop(false);
                if let Some(cwd) = &config.cwd {
                    command.current_dir(cwd);
                }
                #[cfg(windows)]
                command.creation_flags(windows_sys::Win32::System::Threading::CREATE_NO_WINDOW);
                // Do not put the desktop app in the MCP client's kill-on-close job.
                // It stays open after this tool call and after the gateway exits.
                let child = command
                    .spawn()
                    .with_context(|| format!("自动启动应用失败：{}", config.command))?;
                log::info!(
                    "[MCP] auto-started desktop app {} (pid {:?})",
                    config.command,
                    child.id()
                );
                state.child = Some(child);
                state.last_launch = Some(Instant::now());
            }
            loop {
                if app_ready(config).await {
                    state.last_launch = None;
                    return Ok(());
                }
                if let Some(child) = state.child.as_mut() {
                    if let Some(status) = child.try_wait().context("检查应用启动状态失败")? {
                        // A successful short-lived launcher may have handed off.
                        if !status.success() {
                            bail!("应用启动后异常退出：{} ({status})", config.command);
                        }
                    }
                }
                sleep(Duration::from_millis(200)).await;
            }
        })
        .await
        .with_context(|| {
            format!(
                "等待应用 MCP 服务就绪超时（{} 秒）：{}；请检查应用内 MCP 是否启用及 portFile 配置",
                budget.as_secs_f64(),
                config.command,
            )
        })?
    }
}

async fn app_ready(config: &AppAutoStartConfig) -> bool {
    // Discovery files survive application exit; require a live loopback listener.
    // Read again each time because both apps select a new port on startup.
    let Ok(contents) = tokio::fs::read_to_string(&config.port_file).await else {
        return false;
    };
    let port = match &config.port_field {
        Some(field) => serde_json::from_str::<serde_json::Value>(&contents)
            .ok()
            .and_then(|v| v.get(field).and_then(|p| p.as_u64()))
            .and_then(|n| u16::try_from(n).ok()),
        None => contents.trim().parse::<u16>().ok(),
    };
    let Some(port) = port.filter(|p| *p != 0) else {
        return false;
    };
    matches!(
        timeout(Duration::from_millis(300), TcpStream::connect(("127.0.0.1", port))).await,
        Ok(Ok(_))
    )
}
