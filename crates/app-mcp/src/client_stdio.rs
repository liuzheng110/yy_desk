//! Client for external **stdio** MCP servers.
//!
//! The gateway spawns the configured command as a child process, performs the
//! MCP handshake (initialize + notifications/initialized), then talks newline-
//! delimited JSON-RPC over the process pipes.

use std::time::Duration;

use anyhow::{bail, Context};
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::process::{Child, ChildStdin, Command};
use tokio::time::timeout;

use crate::config::McpServerConfig;
use crate::protocol::ToolDefinition;

const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(15);

pub(crate) static ID_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

pub struct StdioMcpClient {
    child: Child,
    stdin: ChildStdin,
    lines: Lines<BufReader<tokio::process::ChildStdout>>,
    // Closing the job also terminates descendants launched by .cmd wrappers.
    #[cfg(windows)]
    _job: std::os::windows::io::OwnedHandle,
}

impl StdioMcpClient {
    /// Spawn the configured command and perform the MCP handshake.
    pub async fn connect(cfg: &McpServerConfig) -> anyhow::Result<Self> {
        let command = cfg.command.as_ref().context("stdio MCP missing `command`")?;
        let mut cmd = Command::new(command);
        #[cfg(windows)]
        cmd.creation_flags(windows_sys::Win32::System::Threading::CREATE_NO_WINDOW);
        cmd.kill_on_drop(true);
        if let Some(args) = &cfg.args {
            cmd.args(args);
        }
        if let Some(cwd) = &cfg.cwd {
            cmd.current_dir(cwd);
        }
        if let Some(env) = &cfg.env {
            for (k, v) in env {
                cmd.env(k, v);
            }
        }
        cmd.stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::inherit());

        let mut child = cmd.spawn().with_context(|| format!("spawn stdio MCP `{command}`"))?;
        #[cfg(windows)]
        let job = own_process_tree(&child).context("track stdio MCP process tree")?;
        let stdin = child.stdin.take().context("stdio MCP stdin unavailable")?;
        let stdout = child.stdout.take().context("stdio MCP stdout unavailable")?;

        let mut client = Self {
            child,
            stdin,
            lines: BufReader::new(stdout).lines(),
            #[cfg(windows)]
            _job: job,
        };
        client.initialize().await?;
        Ok(client)
    }

    /// Raw JSON-RPC request with an auto-incrementing id; resolves the matching response.
    async fn request(&mut self, method: &str, params: Value) -> anyhow::Result<Value> {
        let id = ID_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let req = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
        self.stdin.write_all(req.to_string().as_bytes()).await?;
        self.stdin.write_all(b"\n").await?;
        self.stdin.flush().await?;

        let deadline = timeout(HANDSHAKE_TIMEOUT, async {
            loop {
                let line = self
                    .lines
                    .next_line()
                    .await?
                    .context("stdio MCP closed stdout while awaiting response")?;
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                let v: Value = serde_json::from_str(line).context("stdio MCP returned non-JSON line")?;
                // ignore notifications / unrelated ids
                if v.get("id").and_then(Value::as_u64) != Some(id) {
                    continue;
                }
                if let Some(err) = v.get("error").and_then(Value::as_object) {
                    let code = err.get("code").and_then(Value::as_i64).unwrap_or(-1);
                    let msg = err
                        .get("message")
                        .and_then(Value::as_str)
                        .unwrap_or("unknown error")
                        .to_string();
                    bail!("MCP `{method}` error {code}: {msg}");
                }
                return Ok(v.get("result").cloned().unwrap_or(Value::Null));
            }
        });
        deadline
            .await
            .with_context(|| format!("stdio MCP `{method}` timed out"))?
    }

    /// MCP handshake: initialize, then fire the initialized notification.
    async fn initialize(&mut self) -> anyhow::Result<()> {
        self.request(
            "initialize",
            json!({
                "protocolVersion": crate::protocol::PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": {"name": crate::protocol::SERVER_NAME, "version": env!("CARGO_PKG_VERSION")}
            }),
        )
        .await?;
        let notif = json!({"jsonrpc": "2.0", "method": "notifications/initialized"});
        self.stdin.write_all(notif.to_string().as_bytes()).await?;
        self.stdin.write_all(b"\n").await?;
        self.stdin.flush().await?;
        Ok(())
    }

    pub async fn list_tools(&mut self) -> anyhow::Result<Vec<ToolDefinition>> {
        let res = self.request("tools/list", json!({})).await?;
        let tools = res
            .get("tools")
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(|t| serde_json::from_value::<ToolDefinition>(t.clone()).ok())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        Ok(tools)
    }

    pub async fn call_tool(&mut self, name: &str, arguments: Value) -> anyhow::Result<Value> {
        let res = self
            .request("tools/call", json!({"name": name, "arguments": arguments}))
            .await?;
        Ok(res)
    }

    pub fn child_id(&self) -> Option<u32> {
        self.child.id()
    }
}

impl Drop for StdioMcpClient {
    fn drop(&mut self) {
        // kill()/wait() return futures and do nothing when discarded in Drop.
        // start_kill is synchronous; Tokio reaps the child on handle drop.
        let _ = self.child.start_kill();
    }
}

#[cfg(windows)]
fn own_process_tree(child: &Child) -> anyhow::Result<std::os::windows::io::OwnedHandle> {
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation, SetInformationJobObject,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };

    // SAFETY: the unnamed job handle is uniquely owned, the information buffer
    // has the required layout/size, and the child handle remains borrowed/live.
    unsafe {
        let raw = CreateJobObjectW(std::ptr::null(), std::ptr::null());
        if raw.is_null() {
            return Err(std::io::Error::last_os_error().into());
        }
        let job = OwnedHandle::from_raw_handle(raw);
        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        if SetInformationJobObject(
            job.as_raw_handle(),
            JobObjectExtendedLimitInformation,
            &info as *const _ as *const _,
            std::mem::size_of_val(&info) as u32,
        ) == 0
        {
            return Err(std::io::Error::last_os_error().into());
        }
        let process = child.raw_handle().context("stdio MCP already exited")?;
        if AssignProcessToJobObject(job.as_raw_handle(), process) == 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        Ok(job)
    }
}
