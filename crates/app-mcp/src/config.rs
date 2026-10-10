//! MCP registry configuration (`mcp-servers.json`) and the persisted tool cache.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::Context;
use serde::{Deserialize, Serialize};

use crate::protocol::ToolDefinition;

/// Default TCP port for the gateway HTTP MCP endpoint (127.0.0.1 only).
pub const DEFAULT_MCP_PORT: u16 = 9876;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerConfig {
    /// Display / prefix name, e.g. `filesystem`.
    pub name: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// `"stdio"` or `"http"`.
    pub transport: String,
    // --- stdio transport ---
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub args: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub env: Option<BTreeMap<String, String>>,
    // --- http transport ---
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub headers: Option<BTreeMap<String, String>>,
    // --- common ---
    /// Tool name prefix (`{prefix}__{tool}`) exposed to Codex. Defaults to `name`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_prefix: Option<String>,
    /// Per-call timeout for a tool invocation. Defaults to 60s.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_timeout_ms: Option<u64>,
    /// Circuit-breaker override: max consecutive failures before the server
    /// is temporarily disabled. Defaults to `FailsafeConfig::failure_threshold`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure_threshold: Option<u32>,
    /// Circuit-breaker override: seconds the server stays disabled once the
    /// threshold is hit. Defaults to `FailsafeConfig::reset_timeout_sec`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reset_timeout_sec: Option<u64>,
    /// Optional desktop app to start before a live tool call or manual refresh.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_start: Option<AppAutoStartConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppAutoStartConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    pub cwd: Option<String>,
    /// Plain-text port file (DBX), or JSON discovery file (Netcatty).
    pub port_file: String,
    /// Omit for a plain-text port file; use "port" for Netcatty discovery JSON.
    pub port_field: Option<String>,
    #[serde(default = "default_app_startup_timeout")]
    pub startup_timeout_ms: u64,
}

fn default_app_startup_timeout() -> u64 { 20_000 }

fn default_true() -> bool {
    true
}

/// Circuit-breaker / failsafe parameters (borrowed from mcp-gateway's
/// failsafe design, scaled down for the desktop gateway).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FailsafeConfig {
    /// Consecutive failures that trip the breaker. Default 5.
    #[serde(default = "default_failure_threshold")]
    pub failure_threshold: u32,
    /// Seconds a tripped breaker stays open before auto-recovery. Default 30.
    #[serde(default = "default_reset_timeout")]
    pub reset_timeout_sec: u64,
    /// Max warm-start retry attempts per server before giving up. Default 6.
    #[serde(default = "default_warm_retries")]
    pub warm_start_retries: u32,
}

fn default_failure_threshold() -> u32 {
    5
}
fn default_reset_timeout() -> u64 {
    30
}
fn default_warm_retries() -> u32 {
    6
}

impl Default for FailsafeConfig {
    fn default() -> Self {
        Self {
            failure_threshold: default_failure_threshold(),
            reset_timeout_sec: default_reset_timeout(),
            warm_start_retries: default_warm_retries(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct McpRegistryConfig {
    /// Gateway HTTP port (127.0.0.1). Default 9876.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    #[serde(default)]
    pub servers: Vec<McpServerConfig>,
    /// Meta-mode: expose a small fixed set of `gateway_*` tools instead of the
    /// full backend tool catalog, so clients search-and-invoke on demand and
    /// keep context small when many backends are attached. Default false.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meta_mode: Option<bool>,
    /// Auto warm-start: on gateway start, connect every enabled backend and
    /// prefetch its tool list into the persisted cache (retried with backoff).
    /// Default true.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_warm_start: Option<bool>,
    /// Failsafe / circuit-breaker parameters. Defaults apply when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failsafe: Option<FailsafeConfig>,
}

impl McpRegistryConfig {
    pub fn effective_port(&self) -> u16 {
        self.port.unwrap_or(DEFAULT_MCP_PORT)
    }

    pub fn enabled_servers(&self) -> impl Iterator<Item = &McpServerConfig> {
        self.servers.iter().filter(|s| s.enabled)
    }

    pub fn meta_mode(&self) -> bool {
        self.meta_mode.unwrap_or(false)
    }

    pub fn auto_warm_start(&self) -> bool {
        self.auto_warm_start.unwrap_or(true)
    }

    pub fn failsafe(&self) -> FailsafeConfig {
        self.failsafe.clone().unwrap_or_default()
    }
}

/// Persisted tool cache: server name -> tools last successfully scanned.
/// This is what makes the tool list available to Codex even when the
/// backing MCP server is currently offline.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct McpToolsCache {
    #[serde(default)]
    pub servers: BTreeMap<String, Vec<ToolDefinition>>,
    /// RFC3339 timestamp of the last successful refresh.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refreshed_at: Option<String>,
}

pub fn load_config(path: &Path) -> anyhow::Result<McpRegistryConfig> {
    if !path.exists() {
        return Ok(McpRegistryConfig::default());
    }
    let text = std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let cfg: McpRegistryConfig =
        serde_json::from_str(&text).with_context(|| format!("parse {}", path.display()))?;
    Ok(cfg)
}

pub fn load_cache(path: &Path) -> anyhow::Result<McpToolsCache> {
    if !path.exists() {
        return Ok(McpToolsCache::default());
    }
    let text = std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let cache: McpToolsCache =
        serde_json::from_str(&text).with_context(|| format!("parse {}", path.display()))?;
    Ok(cache)
}

pub fn save_cache(path: &Path, cache: &McpToolsCache) -> anyhow::Result<()> {
    let text = serde_json::to_string_pretty(cache)?;
    std::fs::write(path, text).with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_example_config() {
        let text = r#"{
            "port": 9876,
            "servers": [
                {"name": "fs", "enabled": true, "transport": "stdio", "command": "npx", "args": ["-y", "@modelcontextprotocol/server-filesystem", "./"]},
                {"name": "remote", "enabled": true, "transport": "http", "url": "http://127.0.0.1:9000/mcp"}
            ]
        }"#;
        let cfg: McpRegistryConfig = serde_json::from_str(text).unwrap();
        assert_eq!(cfg.effective_port(), 9876);
        assert_eq!(cfg.servers.len(), 2);
        assert_eq!(cfg.enabled_servers().count(), 2);
    }
}
