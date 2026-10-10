//! MCP gateway crate: aggregate external MCP servers for Codex.
//!
//! Architecture:
//! ```text
//! Codex ── (stdio | Streamable HTTP) ──▶ myapp-gateway ──▶ external MCP servers
//!                                        (this crate)     (stdio sub-process or http URL)
//! ```
//! - `tools/list` answers from a **persisted cache**, so tools stay discoverable
//!   even when an external MCP server is offline.
//! - `tools/call` forwards **live** to the owning server; a server started after
//!   Codex becomes usable without restarting the session.

pub mod app_launcher;
pub mod client_http;
pub mod client_stdio;
pub mod config;
pub mod gateway;
pub mod protocol;
pub mod proxy_stdio;
pub mod server_http;

pub use config::{load_config, McpRegistryConfig, McpServerConfig, McpToolsCache, DEFAULT_MCP_PORT};
pub use gateway::{GatewayHandle, RefreshReport};
pub use protocol::{ToolDefinition, PROTOCOL_VERSION};
