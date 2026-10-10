//! Client for external **Streamable HTTP** MCP servers.

use std::time::Duration;

use anyhow::{bail, Context};
use serde_json::{json, Value};
use tokio::time::timeout;

use crate::config::McpServerConfig;
use crate::protocol::{parse_maybe_sse, ToolDefinition};

pub struct HttpMcpClient {
    client: reqwest::Client,
    url: String,
    headers: Vec<(String, String)>,
    session_id: Option<String>,
    timeout_ms: u64,
}

impl HttpMcpClient {
    pub async fn connect(cfg: &McpServerConfig) -> anyhow::Result<Self> {
        let url = cfg.url.clone().context("http MCP missing `url`")?;
        let headers = cfg
            .headers
            .clone()
            .unwrap_or_default()
            .into_iter()
            .collect::<Vec<_>>();
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(120))
            .build()?;
        Ok(Self {
            client,
            url,
            headers,
            session_id: None,
            timeout_ms: cfg.tool_timeout_ms.unwrap_or(60_000),
        })
    }

    async fn post(&mut self, method: &str, params: Value, has_id: bool) -> anyhow::Result<Value> {
        let id = if has_id {
            Some(std::sync::atomic::AtomicU64::fetch_add(
                &crate::client_stdio::ID_COUNTER,
                1,
                std::sync::atomic::Ordering::Relaxed,
            ))
        } else {
            None
        };
        let body = match id {
            Some(id) => json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}),
            None => json!({"jsonrpc": "2.0", "method": method, "params": params}),
        };

        let mut req = self
            .client
            .post(&self.url)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/event-stream");
        for (k, v) in &self.headers {
            req = req.header(k, v);
        }
        if let Some(sid) = &self.session_id {
            req = req.header("Mcp-Session-Id", sid);
        }

        let resp = req.json(&body).send().await.with_context(|| format!("POST {}", self.url))?;
        if let Some(sid) = resp.headers().get("mcp-session-id") {
            self.session_id = Some(sid.to_str()?.to_string());
        }
        let status = resp.status();
        let text = resp.text().await?;

        if !has_id {
            // notification: nothing to return
            return Ok(Value::Null);
        }
        if !status.is_success() {
            bail!("HTTP MCP `{method}` status {status}: {}", text.chars().take(200).collect::<String>());
        }
        let v = parse_maybe_sse(&text);
        if let Some(err) = v.get("error").and_then(Value::as_object) {
            let code = err.get("code").and_then(Value::as_i64).unwrap_or(-1);
            let msg = err
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("unknown error")
                .to_string();
            bail!("MCP `{method}` error {code}: {msg}");
        }
        Ok(v.get("result").cloned().unwrap_or(Value::Null))
    }

    pub(crate) async fn initialize(&mut self) -> anyhow::Result<()> {
        self.post(
            "initialize",
            json!({
                "protocolVersion": crate::protocol::PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": {"name": crate::protocol::SERVER_NAME, "version": env!("CARGO_PKG_VERSION")}
            }),
            true,
        )
        .await?;
        self.post("notifications/initialized", json!({}), false).await?;
        Ok(())
    }

    pub async fn list_tools(&mut self) -> anyhow::Result<Vec<ToolDefinition>> {
        let res = timeout(Duration::from_millis(self.timeout_ms), self.post("tools/list", json!({}), true))
            .await
            .context("http MCP tools/list timed out")??;
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
        let res = timeout(
            Duration::from_millis(self.timeout_ms),
            self.post("tools/call", json!({"name": name, "arguments": arguments}), true),
        )
        .await
        .context("http MCP tools/call timed out")??;
        Ok(res)
    }
}
