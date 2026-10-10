//! stdio proxy entry (`MyApp.exe --mcp-proxy`).
//!
//! Codex can be configured with a *stdio* MCP launcher pointing at this app:
//! the proxy forwards every JSON-RPC message to the local gateway HTTP
//! endpoint, so the tool list comes from MyApp's MCP registry. If MyApp is not
//! running yet, the proxy spawns it (without `--mcp-proxy`) and waits for the
//! gateway port to come up — so a Codex session started before MyApp still
//! discovers the registered tools.

use std::process::Stdio;
use std::time::Duration;

use anyhow::Context;
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

pub fn run_proxy(port: u16) -> anyhow::Result<()> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;
    rt.block_on(run_proxy_async(port))
}

async fn run_proxy_async(port: u16) -> anyhow::Result<()> {
    let url = format!("http://127.0.0.1:{port}/mcp");

    if !tcp_reachable(port).await {
        log::info!("MCP gateway not reachable on port {port}; spawning MyApp");
        spawn_main_app()?;
        wait_for_gateway(port, Duration::from_secs(20)).await?;
    }

    let client = reqwest::Client::new();
    let mut stdin = BufReader::new(tokio::io::stdin()).lines();
    let mut session: Option<String> = None;

    while let Some(line) = stdin.next_line().await? {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let parsed: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("[myapp-mcp-proxy] dropping invalid JSON: {e}");
                continue;
            }
        };
        let is_notification = parsed.get("id").is_none();

        let mut req = client
            .post(&url)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/event-stream");
        if let Some(sid) = &session {
            req = req.header("Mcp-Session-Id", sid);
        }

        match req.json(&parsed).send().await {
            Ok(resp) => {
                if let Some(sid) = resp.headers().get("mcp-session-id") {
                    session = Some(sid.to_str().unwrap_or("").to_string());
                }
                if is_notification {
                    continue;
                }
                let text = resp.text().await.unwrap_or_default();
                let out = crate::protocol::parse_maybe_sse(&text);
                let mut stdout = tokio::io::stdout();
                stdout.write_all(out.to_string().as_bytes()).await?;
                stdout.write_all(b"\n").await?;
                stdout.flush().await?;
            }
            Err(e) => {
                // respond with an MCP error so Codex doesn't hang waiting
                if !is_notification {
                    let id = parsed.get("id").cloned().unwrap_or(Value::Null);
                    let err = crate::protocol::error_response(
                        id,
                        crate::protocol::JSONRPC_INTERNAL_ERROR,
                        format!("gateway unreachable: {e}"),
                    );
                    let mut stdout = tokio::io::stdout();
                    stdout.write_all(serde_json::to_string(&err)?.as_bytes()).await?;
                    stdout.write_all(b"\n").await?;
                    stdout.flush().await?;
                }
            }
        }
    }
    Ok(())
}

async fn tcp_reachable(port: u16) -> bool {
    tokio::net::TcpStream::connect(("127.0.0.1", port)).await.is_ok()
}

async fn wait_for_gateway(port: u16, timeout: Duration) -> anyhow::Result<()> {
    let deadline = tokio::time::Instant::now() + timeout;
    while tokio::time::Instant::now() < deadline {
        if tcp_reachable(port).await {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(300)).await;
    }
    anyhow::bail!("MyApp gateway did not come up within {timeout:?}")
}

fn spawn_main_app() -> anyhow::Result<()> {
    let exe = std::env::current_exe().context("resolve current exe")?;
    let args = std::env::args()
        .skip(1)
        .filter(|a| a != "--mcp-proxy")
        .collect::<Vec<_>>();
    log::info!("spawning MyApp main: {} {:?}", exe.display(), args);
    let mut cmd = std::process::Command::new(&exe);
    cmd.args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    // detached so the main app outlives the proxy if Codex kills it
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        cmd.creation_flags(CREATE_NO_WINDOW | DETACHED_PROCESS);
    }
    cmd.spawn().context("spawn MyApp main process")?;
    Ok(())
}
