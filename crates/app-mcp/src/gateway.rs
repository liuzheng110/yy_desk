//! MCP gateway core: registry + persisted tool cache + aggregation + routing,
//! plus backend lifecycle (warm-start / circuit breaker) and an optional
//! meta-mode surface borrowed from mcp-gateway:
//!
//! - **warm-start**: on startup, connect every enabled backend and prefetch its
//!   tool list into the persisted cache (retried with backoff). This removes
//!   the need to manually refresh before first use.
//! - **circuit breaker**: consecutive failures temporarily disable a backend;
//!   it recovers automatically after a reset timeout.
//! - **meta-mode**: instead of exposing the full tool catalog, expose a small
//!   fixed set of `gateway_*` meta-tools so clients search-and-invoke on
//!   demand, keeping context small when many backends are attached.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use anyhow::{bail, Context};
use serde_json::{json, Value};
use tokio::sync::Mutex;

use crate::client_http::HttpMcpClient;
use crate::app_launcher::AppLauncher;
use crate::client_stdio::StdioMcpClient;
use crate::config::{
    load_cache, load_config, save_cache, FailsafeConfig, McpRegistryConfig, McpServerConfig,
    McpToolsCache,
};
use crate::protocol::ToolDefinition;

/// One tool as exposed to Codex: `name` carries the `{prefix}__{tool}` form.
#[derive(Debug, Clone)]
pub struct AggregatedTool {
    /// Original (unprefixed) tool definition.
    pub tool: ToolDefinition,
    /// Fully-qualified name sent to Codex, e.g. `filesystem__read_file`.
    pub qualified_name: String,
    /// Owning server config name.
    pub server: String,
}

#[derive(Debug, Clone)]
pub struct RefreshReport {
    pub server: String,
    pub ok: bool,
    pub tool_count: usize,
    pub error: Option<String>,
}

/// Backend lifecycle status (subset of mcp-gateway's state machine).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendStatus {
    /// Not yet probed (initial state).
    Unknown,
    /// Warm-start / last call succeeded; routing is allowed.
    Active,
    /// Warm-start or calls have failed; routing is allowed but failing.
    Failed,
    /// Circuit breaker tripped; calls are rejected until reset timeout.
    CircuitOpen,
    /// Operator-disabled via kill switch; calls are rejected until revive.
    Killed,
}

impl BackendStatus {
    pub fn label(&self) -> &'static str {
        match self {
            BackendStatus::Unknown => "unknown",
            BackendStatus::Active => "active",
            BackendStatus::Failed => "failed",
            BackendStatus::CircuitOpen => "circuitOpen",
            BackendStatus::Killed => "killed",
        }
    }
}

/// Per-backend runtime state (in-memory only).
#[derive(Debug, Clone)]
pub struct BackendRuntime {
    pub status: BackendStatus,
    pub consecutive_failures: u32,
    pub last_error: Option<String>,
    pub circuit_open_until: Option<Instant>,
}

/// Thread-safe handle shared by the HTTP gateway server and Tauri commands.
#[derive(Clone)]
pub struct GatewayHandle {
    inner: Arc<Mutex<McpGatewayInner>>,
    app_launcher: AppLauncher,
}

struct McpGatewayInner {
    config: McpRegistryConfig,
    cache: McpToolsCache,
    runtimes: BTreeMap<String, BackendRuntime>,
    /// Invocation counts per qualified tool name (for search usage boost).
    usages: BTreeMap<String, u32>,
    config_path: PathBuf,
    cache_path: PathBuf,
}

impl GatewayHandle {
    pub fn load(config_path: &Path, cache_path: &Path) -> anyhow::Result<Self> {
        let config = load_config(config_path)?;
        let cache = load_cache(cache_path)?;
        Ok(Self::new(config, cache, config_path.to_path_buf(), cache_path.to_path_buf()))
    }

    fn new(
        config: McpRegistryConfig,
        cache: McpToolsCache,
        config_path: PathBuf,
        cache_path: PathBuf,
    ) -> Self {
        let mut runtimes = BTreeMap::new();
        for s in config.enabled_servers() {
            runtimes.insert(
                s.name.clone(),
                BackendRuntime {
                    status: BackendStatus::Unknown,
                    consecutive_failures: 0,
                    last_error: None,
                    circuit_open_until: None,
                },
            );
        }
        Self {
            app_launcher: AppLauncher::default(),
            inner: Arc::new(Mutex::new(McpGatewayInner {
                config,
                cache,
                runtimes,
                usages: BTreeMap::new(),
                config_path,
                cache_path,
            })),
        }
    }

    pub async fn port(&self) -> u16 {
        self.inner.lock().await.config.effective_port()
    }

    pub async fn meta_mode(&self) -> bool {
        self.inner.lock().await.config.meta_mode()
    }

    pub async fn auto_warm_start(&self) -> bool {
        self.inner.lock().await.config.auto_warm_start()
    }

    pub async fn reload(&self) -> anyhow::Result<()> {
        let mut inner = self.inner.lock().await;
        let new_config = load_config(&inner.config_path)?;
        let mut runtimes = BTreeMap::new();
        for s in new_config.enabled_servers() {
            let prev = inner.runtimes.get(&s.name);
            runtimes.insert(
                s.name.clone(),
                BackendRuntime {
                    status: BackendStatus::Unknown,
                    consecutive_failures: 0,
                    last_error: None,
                    circuit_open_until: None,
                },
            );
            let _ = prev;
        }
        inner.config = new_config;
        inner.runtimes = runtimes;
        Ok(())
    }

    /// Server list + per-server runtime state, for UI/CLI introspection.
    pub async fn server_status(&self) -> Value {
        let inner = self.inner.lock().await;
        let servers: Vec<Value> = inner
            .config
            .servers
            .iter()
            .map(|s| {
                let tools = inner.cache.servers.get(&s.name).map(|v| v.len()).unwrap_or(0);
                let rt = inner.runtimes.get(&s.name);
                json!({
                    "name": s.name,
                    "enabled": s.enabled,
                    "transport": s.transport,
                    "toolPrefix": s.tool_prefix.clone().unwrap_or_else(|| s.name.clone()),
                    "command": s.command,
                    "url": s.url,
                    "cachedTools": tools,
                    "status": rt.map(|r| r.status.label()).unwrap_or("unknown"),
                    "consecutiveFailures": rt.map(|r| r.consecutive_failures).unwrap_or(0),
                    "lastError": rt.and_then(|r| r.last_error.clone()),
                    "autoStart": s.auto_start.as_ref().is_some_and(|a| a.enabled),
                })
            })
            .collect();
        json!({
            "port": inner.config.effective_port(),
            "servers": servers,
            "refreshedAt": inner.cache.refreshed_at,
            "metaMode": inner.config.meta_mode(),
            "autoWarmStart": inner.config.auto_warm_start(),
        })
    }

    /// Merged tool list from the **cache** only — available even when the
    /// backing MCP servers are offline. This is the property that lets Codex
    /// discover tools without requiring every external MCP to be running.
    pub async fn list_all_tools(&self) -> Vec<AggregatedTool> {
        let inner = self.inner.lock().await;
        let mut out = Vec::new();
        for server in inner.config.enabled_servers() {
            let prefix = server.tool_prefix.clone().unwrap_or_else(|| server.name.clone());
            if let Some(tools) = inner.cache.servers.get(&server.name) {
                for t in tools {
                    out.push(AggregatedTool {
                        tool: t.clone(),
                        qualified_name: format!("{prefix}__{}", t.name),
                        server: server.name.clone(),
                    });
                }
            }
        }
        out
    }

    /// Tool list restricted to a single named server, used by the `?scope=`
    /// HTTP variant so Codex can expose each MCP as its own entry with a
    /// small, focused tool catalog.
    pub async fn list_tools_for(&self, scope: &str) -> Vec<AggregatedTool> {
        let inner = self.inner.lock().await;
        let mut out = Vec::new();
        for server in inner.config.enabled_servers() {
            if server.name != scope {
                continue;
            }
            let prefix = server.tool_prefix.clone().unwrap_or_else(|| server.name.clone());
            if let Some(tools) = inner.cache.servers.get(&server.name) {
                for t in tools {
                    out.push(AggregatedTool {
                        tool: t.clone(),
                        qualified_name: format!("{prefix}__{}", t.name),
                        server: server.name.clone(),
                    });
                }
            }
        }
        out
    }

    /// Fixed meta-tool definitions exposed when `meta_mode` is on.
    pub async fn meta_tools(&self) -> Vec<ToolDefinition> {
        let _ = self.inner.lock().await;
        meta_tool_defs()
    }

    /// Route a call by its `{prefix}__{tool}` name and forward it **live** to
    /// the owning external MCP server. Circuit breaker and usage stats apply.
    pub async fn call_tool(&self, qualified_name: &str, arguments: Value) -> anyhow::Result<Value> {
        let inner = self.inner.lock().await;
        let (server_ref, tool_name) = resolve_server(&inner.config, qualified_name)?;
        let server = server_ref.clone();
        let tool_name = tool_name.clone();
        let qname = qualified_name.to_string();
        drop(inner);
        self.dispatch(&server, &tool_name, arguments, Some(&qname)).await
    }

    /// Invoke a tool by raw server name + tool name (used by `gateway_invoke`).
    pub async fn call_tool_by_server(
        &self,
        server_name: &str,
        tool_name: &str,
        arguments: Value,
    ) -> anyhow::Result<Value> {
        let inner = self.inner.lock().await;
        let server = inner
            .config
            .servers
            .iter()
            .find(|s| s.name == server_name && s.enabled)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("no enabled MCP server named `{server_name}`"))?;
        let qname = format!("{}__{}", server_name, tool_name);
        drop(inner);
        self.dispatch(&server, tool_name, arguments, Some(&qname)).await
    }

    /// Shared dispatch path: circuit breaker check -> live forwarding ->
    /// failure counting / usage recording.
    async fn dispatch(
        &self,
        server: &McpServerConfig,
        tool_name: &str,
        arguments: Value,
        qname: Option<&str>,
    ) -> anyhow::Result<Value> {
        let (threshold, reset_secs) = {
            let inner = self.inner.lock().await;
            let failsafe = inner.config.failsafe();
            (
                server.failure_threshold.unwrap_or(failsafe.failure_threshold),
                server.reset_timeout_sec.unwrap_or(failsafe.reset_timeout_sec),
            )
        };

        // Circuit breaker gate.
        {
            let inner = self.inner.lock().await;
            if let Some(rt) = inner.runtimes.get(&server.name) {
                match rt.status {
                    BackendStatus::Killed => {
                        bail!(
                            "backend `{}` is disabled by kill switch; revive it first",
                            server.name
                        )
                    }
                    BackendStatus::CircuitOpen => {
                        if let Some(until) = rt.circuit_open_until {
                            if until > Instant::now() {
                                let remain = until.saturating_duration_since(Instant::now()).as_secs();
                                bail!(
                                    "backend `{}` circuit is open ({}s left); call later or revive manually",
                                    server.name, remain
                                );
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        let timeout_ms = server.tool_timeout_ms.unwrap_or(60_000);
        // The whole connect+call sequence is one fallible unit so that connect
        // failures also feed the circuit breaker.
        let outcome: Result<Value, anyhow::Error> = async {
            // Start/wait before sending the user's tool request. Never replay a
            // tools/call after failure: it could already have performed a write.
            self.app_launcher.ensure_ready(server.auto_start.as_ref()).await?;
            match server.transport.as_str() {
                "stdio" => {
                    let mut client = StdioMcpClient::connect(server).await?;
                    tokio::time::timeout(
                        Duration::from_millis(timeout_ms),
                        client.call_tool(tool_name, arguments),
                    )
                    .await
                    .context("stdio tools/call timed out")?
                }
                "http" => {
                    let mut client = HttpMcpClient::connect(server).await?;
                    client.initialize().await?;
                    tokio::time::timeout(
                        Duration::from_millis(timeout_ms),
                        client.call_tool(tool_name, arguments),
                    )
                    .await
                    .context("http tools/call timed out")?
                }
                other => bail!("unsupported transport `{other}` for server `{}`", server.name),
            }
        }
        .await;

        match outcome {
            Ok(result) => {
                let mut inner = self.inner.lock().await;
                if let Some(rt) = inner.runtimes.get_mut(&server.name) {
                    rt.status = BackendStatus::Active;
                    rt.consecutive_failures = 0;
                    rt.last_error = None;
                    rt.circuit_open_until = None;
                }
                if let Some(q) = qname {
                    *inner.usages.entry(q.to_string()).or_insert(0) += 1;
                }
                Ok(result)
            }
            Err(e) => {
                let msg = e.to_string();
                let mut inner = self.inner.lock().await;
                let mut open = false;
                if let Some(rt) = inner.runtimes.get_mut(&server.name) {
                    rt.consecutive_failures += 1;
                    rt.last_error = Some(msg.clone());
                    rt.status = BackendStatus::Failed;
                    if rt.consecutive_failures >= threshold {
                        rt.status = BackendStatus::CircuitOpen;
                        rt.circuit_open_until = Some(Instant::now() + Duration::from_secs(reset_secs));
                        open = true;
                    }
                }
                if open {
                    bail!(
                        "backend `{}` failed {} times in a row; circuit opened for {}s: {}",
                        server.name,
                        threshold,
                        reset_secs,
                        msg
                    );
                }
                Err(e)
            }
        }
    }

    /// Operator kill switch: immediately disable routing to a backend.
    pub async fn kill_server(&self, server_name: &str) -> anyhow::Result<()> {
        let mut inner = self.inner.lock().await;
        let rt = inner
            .runtimes
            .get_mut(server_name)
            .ok_or_else(|| anyhow::anyhow!("no backend named `{server_name}`"))?;
        rt.status = BackendStatus::Killed;
        rt.circuit_open_until = None;
        Ok(())
    }

    /// Revive a killed (or circuit-open) backend and reset its error budget.
    pub async fn revive_server(&self, server_name: &str) -> anyhow::Result<()> {
        let mut inner = self.inner.lock().await;
        let rt = inner
            .runtimes
            .get_mut(server_name)
            .ok_or_else(|| anyhow::anyhow!("no backend named `{server_name}`"))?;
        rt.status = BackendStatus::Active;
        rt.consecutive_failures = 0;
        rt.last_error = None;
        rt.circuit_open_until = None;
        Ok(())
    }

    /// Keyword search across all cached tools with ranked results.
    /// Usage-boosted: frequently used tools score higher, but relevance wins.
    pub async fn search_tools(&self, query: &str, limit: Option<usize>) -> Vec<Value> {
        let inner = self.inner.lock().await;
        let terms: Vec<String> = query
            .to_lowercase()
            .split_whitespace()
            .map(|s| s.to_string())
            .collect();
        let mut scored: Vec<(f64, String, ToolDefinition, String)> = Vec::new();
        for server in inner.config.enabled_servers() {
            let prefix = server.tool_prefix.clone().unwrap_or_else(|| server.name.clone());
            if let Some(tools) = inner.cache.servers.get(&server.name) {
                for t in tools {
                    let qname = format!("{prefix}__{}", t.name);
                    let score = score_text(&terms, &t.name, t.description.as_deref().unwrap_or(""));
                    if score > 0 {
                        let uses = inner.usages.get(&qname).copied().unwrap_or(0);
                        let final_score =
                            score as f64 * (1.0 + (uses as f64 + 1.0).log2() * 0.15);
                        scored.push((final_score, qname, t.clone(), server.name.clone()));
                    }
                }
            }
        }
        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(limit.unwrap_or(10));
        scored
            .into_iter()
            .map(|(score, qname, t, server)| {
                json!({
                    "name": qname,
                    "server": server,
                    "description": t.description,
                    "score": (score * 100.0).round() as u64 / 100,
                })
            })
            .collect()
    }

    /// Handle a `gateway_*` meta-tool call.
    pub async fn handle_meta_call(&self, method: &str, params: Value) -> anyhow::Result<Value> {
        match method {
            "gateway_search_tools" => {
                let query = params.get("query").and_then(Value::as_str).unwrap_or("");
                let limit = params.get("limit").and_then(Value::as_u64).map(|v| v as usize);
                let results = self.search_tools(query, limit).await;
                Ok(json!({
                    "content": [{"type": "text", "text": serde_json::to_string_pretty(&results)?}]
                }))
            }
            "gateway_list_tools" => {
                let server_filter = params.get("server").and_then(Value::as_str);
                let mut out = Vec::new();
                for a in self.list_all_tools().await {
                    if server_filter.map(|s| a.server == s).unwrap_or(true) {
                        out.push(json!({
                            "name": a.qualified_name,
                            "server": a.server,
                            "description": a.tool.description,
                        }));
                    }
                }
                Ok(json!({
                    "content": [{"type": "text", "text": serde_json::to_string_pretty(&out)?}]
                }))
            }
            "gateway_list_servers" => {
                let status = self.server_status().await;
                Ok(json!({
                    "content": [{"type": "text", "text": serde_json::to_string_pretty(&status)?}]
                }))
            }
            "gateway_invoke" => {
                let server = params
                    .get("server")
                    .and_then(Value::as_str)
                    .ok_or_else(|| anyhow::anyhow!("gateway_invoke requires `server`"))?;
                let tool = params
                    .get("tool")
                    .and_then(Value::as_str)
                    .ok_or_else(|| anyhow::anyhow!("gateway_invoke requires `tool`"))?;
                let arguments = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
                let result = self.call_tool_by_server(server, tool, arguments).await?;
                let payload = if result.get("content").is_some() || result.get("structuredContent").is_some() {
                    result
                } else {
                    json!({"content": [{"type": "text", "text": result.to_string()}]})
                };
                Ok(payload)
            }
            other => bail!("unknown meta tool `{other}`"),
        }
    }

    /// Re-scan every enabled server, updating the persisted tool cache.
    /// Failed servers keep their previous cache entry.
    pub async fn refresh_tools(&self) -> Vec<RefreshReport> {
        let mut reports = Vec::new();
        let inner = self.inner.lock().await;
        let servers = inner.config.enabled_servers().cloned().collect::<Vec<_>>();
        drop(inner);

        let mut updated = Vec::new();
        for server in servers {
            let killed = self.inner.lock().await.runtimes.get(&server.name)
                .is_some_and(|r| r.status == BackendStatus::Killed);
            if killed {
                reports.push(RefreshReport {
                    server: server.name.clone(), ok: false, tool_count: 0,
                    error: Some("后端已停用，请先恢复后再刷新".into()),
                });
                continue;
            }
            let outcome = async {
                self.app_launcher.ensure_ready(server.auto_start.as_ref()).await?;
                scan_server(&server).await
            }.await;
            match outcome {
                Ok(tools) => {
                    updated.push((server.name.clone(), tools.clone()));
                    reports.push(RefreshReport {
                        server: server.name.clone(),
                        ok: true,
                        tool_count: tools.len(),
                        error: None,
                    });
                    let mut inner = self.inner.lock().await;
                    if let Some(rt) = inner.runtimes.get_mut(&server.name) {
                        rt.status = BackendStatus::Active;
                        rt.consecutive_failures = 0;
                        rt.last_error = None;
                        rt.circuit_open_until = None;
                    }
                }
                Err(e) => {
                    reports.push(RefreshReport {
                        server: server.name.clone(),
                        ok: false,
                        tool_count: 0,
                        error: Some(e.to_string()),
                    });
                    let mut inner = self.inner.lock().await;
                    if let Some(rt) = inner.runtimes.get_mut(&server.name) {
                        rt.status = BackendStatus::Failed;
                        rt.last_error = Some(e.to_string());
                    }
                }
            }
        }

        let mut inner = self.inner.lock().await;
        for (name, tools) in updated {
            inner.cache.servers.insert(name, tools);
        }
        inner.cache.refreshed_at = Some(chrono_utc_now());
        if let Err(e) = save_cache(&inner.cache_path, &inner.cache) {
            log::warn!("failed to persist MCP tool cache: {e:#}");
        }
        reports
    }

    /// Auto warm-start: connect every enabled backend and prefetch its tool
    /// list into the persisted cache, retrying with exponential backoff
    /// (2s -> 30s cap). Spawned by the host (Tauri) at startup.
    pub async fn run_warm_start(&self) {
        let servers: Vec<McpServerConfig> = {
            let inner = self.inner.lock().await;
            inner.config.enabled_servers().cloned().collect()
        };
        let failsafe: FailsafeConfig = {
            let inner = self.inner.lock().await;
            inner.config.failsafe()
        };
        for server in servers {
            let mut delay = Duration::from_secs(2);
            let mut attempts = 0u32;
            let mut done = false;
            while !done {
                match scan_server(&server).await {
                    Ok(tools) => {
                        let mut inner = self.inner.lock().await;
                        inner.cache.servers.insert(server.name.clone(), tools);
                        inner.cache.refreshed_at = Some(chrono_utc_now());
                        if let Err(e) = save_cache(&inner.cache_path, &inner.cache) {
                            log::warn!("failed to persist MCP tool cache: {e:#}");
                        }
                        if let Some(rt) = inner.runtimes.get_mut(&server.name) {
                            rt.status = BackendStatus::Active;
                            rt.consecutive_failures = 0;
                            rt.last_error = None;
                            rt.circuit_open_until = None;
                        }
                        log::info!(
                            "[MCP] warm-start succeeded for `{}` ({} tools)",
                            server.name,
                            inner.cache.servers.get(&server.name).map(|v| v.len()).unwrap_or(0)
                        );
                        done = true;
                    }
                    Err(e) => {
                        attempts += 1;
                        let msg = e.to_string();
                        {
                            let mut inner = self.inner.lock().await;
                            if let Some(rt) = inner.runtimes.get_mut(&server.name) {
                                rt.status = BackendStatus::Failed;
                                rt.last_error = Some(msg.clone());
                            }
                        }
                        if attempts >= failsafe.warm_start_retries {
                            log::warn!(
                                "[MCP] warm-start gave up on `{}` after {attempts} attempts: {msg}",
                                server.name
                            );
                            done = true;
                        } else {
                            log::info!(
                                "[MCP] warm-start retry {attempts} for `{}` in {:?}: {msg}",
                                server.name,
                                delay
                            );
                            tokio::time::sleep(delay).await;
                            delay = (delay * 2).min(Duration::from_secs(30));
                        }
                    }
                }
            }
        }
    }
}

/// Fixed meta-tool definitions for meta-mode.
fn meta_tool_defs() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "gateway_search_tools".to_string(),
            description: Some("Search all connected MCP backends for tools matching a keyword query. Use this first to discover which tools are available before calling one.".to_string()),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": {"type": "string", "description": "Search keywords, e.g. \"weather search\""},
                    "limit": {"type": "number", "description": "Max results, default 10"}
                },
                "required": ["query"]
            }),
        },
        ToolDefinition {
            name: "gateway_list_tools".to_string(),
            description: Some("List tools from one backend (or all backends). Uses cached tool lists, so it works even when a backend is offline.".to_string()),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "server": {"type": "string", "description": "Backend name; omit to list all"}
                }
            }),
        },
        ToolDefinition {
            name: "gateway_list_servers".to_string(),
            description: Some("List all configured MCP backends with their status (active / failed / circuit open / killed) and cached tool counts.".to_string()),
            input_schema: json!({ "type": "object", "properties": {} }),
        },
        ToolDefinition {
            name: "gateway_invoke".to_string(),
            description: Some("Invoke a tool on any configured MCP backend. Use gateway_search_tools first to find the server and tool name.".to_string()),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "server": {"type": "string", "description": "Backend name, e.g. \"tavily\""},
                    "tool": {"type": "string", "description": "Tool name without prefix, e.g. \"web_search\""},
                    "arguments": {"type": "object", "description": "Tool arguments"}
                },
                "required": ["server", "tool"]
            }),
        },
    ]
}

/// Simple ranked relevance scorer (subset of mcp-gateway's tiers):
/// - all query words inside the tool name: 15 (strongest)
/// - all query words inside name OR description: 10
/// - partial words matched: 2 + 2*matched
/// - nothing matched: 0
fn score_text(terms: &[String], name: &str, description: &str) -> i32 {
    if terms.is_empty() {
        return 0;
    }
    let name_l = name.to_lowercase();
    let desc_l = description.to_lowercase();
    if terms.iter().all(|t| name_l.contains(t.as_str())) {
        return 15;
    }
    if terms
        .iter()
        .all(|t| name_l.contains(t.as_str()) || desc_l.contains(t.as_str()))
    {
        return 10;
    }
    let matched = terms
        .iter()
        .filter(|t| name_l.contains(t.as_str()) || desc_l.contains(t.as_str()))
        .count();
    if matched > 0 {
        return 2 + 2 * matched as i32;
    }
    0
}

async fn scan_server(server: &McpServerConfig) -> anyhow::Result<Vec<ToolDefinition>> {
    match server.transport.as_str() {
        "stdio" => {
            let mut client = StdioMcpClient::connect(server).await?;
            client.list_tools().await
        }
        "http" => {
            let mut client = HttpMcpClient::connect(server).await?;
            client.initialize().await?;
            client.list_tools().await
        }
        other => bail!("unsupported transport `{other}`"),
    }
}

/// Find the server whose `tool_prefix` matches the qualified name (longest
/// prefix wins) and return the unprefixed tool name.
fn resolve_server<'a>(
    config: &'a McpRegistryConfig,
    qualified_name: &str,
) -> anyhow::Result<(&'a McpServerConfig, String)> {
    let mut best: Option<(&McpServerConfig, &str)> = None;
    for server in config.enabled_servers() {
        let prefix = server.tool_prefix.clone().unwrap_or_else(|| server.name.clone());
        let marker = format!("{prefix}__");
        if qualified_name.starts_with(&marker) {
            let rest = &qualified_name[marker.len()..];
            if !rest.is_empty() {
                let better = match best {
                    None => true,
                    Some((prev, _)) => {
                        prefix.len()
                            > prev.tool_prefix
                                .clone()
                                .unwrap_or_else(|| prev.name.clone())
                                .len()
                    }
                };
                if better {
                    best = Some((server, rest));
                }
            }
        }
    }
    match best {
        Some((server, tool)) => Ok((server, tool.to_string())),
        None => bail!("no configured MCP server owns tool `{qualified_name}`"),
    }
}

fn chrono_utc_now() -> String {
    let d = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    format!("{}", d.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::McpRegistryConfig;

    #[test]
    fn resolve_prefix_works() {
        let cfg: McpRegistryConfig = serde_json::from_str(
            r#"{
                "servers": [
                    {"name": "fs", "enabled": true, "transport": "stdio", "command": "x"},
                    {"name": "fs-extra", "enabled": true, "transport": "http", "url": "http://x"}
                ]
            }"#,
        )
        .unwrap();
        let (server, tool) = resolve_server(&cfg, "fs__read_file").unwrap();
        assert_eq!(server.name, "fs");
        assert_eq!(tool, "read_file");
        let (server, tool) = resolve_server(&cfg, "fs-extra__do_stuff").unwrap();
        assert_eq!(server.name, "fs-extra");
        assert_eq!(tool, "do_stuff");
        assert!(resolve_server(&cfg, "unknown__x").is_err());
    }

    #[test]
    fn score_text_tiers() {
        // all words in name -> 15
        assert_eq!(score_text(&["read".into(), "file".into()], "read_file", "reads a file"), 15);
        // all words in name+description -> 10
        assert_eq!(score_text(&["read".into(), "file".into()], "open_doc", "read a file from disk"), 10);
        // single word in description -> 10
        assert_eq!(score_text(&["web".into()], "search", "web search tool"), 10);
        // all words in name -> 15
        assert_eq!(score_text(&["batch".into()], "gmail_batch_modify", "batch modify"), 15);
        // partial words -> 2 + 2*1 = 4
        assert_eq!(score_text(&["foo".into(), "search".into()], "search", "web search"), 4);
        // nothing -> 0
        assert_eq!(score_text(&["zzz".into()], "search", "web search"), 0);
    }

    #[test]
    fn meta_defs_shape() {
        let defs = meta_tool_defs();
        assert_eq!(defs.len(), 4);
        assert!(defs.iter().any(|d| d.name == "gateway_search_tools"));
        assert!(defs.iter().any(|d| d.name == "gateway_invoke"));
        for d in &defs {
            assert!(d.description.as_deref().unwrap_or("").len() > 0);
            assert!(d.input_schema.is_object());
        }
    }
}
