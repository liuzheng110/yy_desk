//! MCP (Model Context Protocol) JSON-RPC structures and shared constants.
//!
//! The gateway speaks the standard MCP protocol (stdio newline-delimited JSON-RPC
//! and Streamable HTTP) toward Codex, and also acts as a *client* toward the
//! configured external MCP servers (stdio sub-processes or Streamable HTTP URLs).

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// Latest stable protocol revision we advertise on `initialize`.
pub const PROTOCOL_VERSION: &str = "2025-06-18";

/// Name reported to Codex on `initialize`.
pub const SERVER_NAME: &str = "myapp-gateway";

/// JSON-RPC error codes.
pub const JSONRPC_PARSE_ERROR: i64 = -32700;
pub const JSONRPC_INVALID_REQUEST: i64 = -32600;
pub const JSONRPC_METHOD_NOT_FOUND: i64 = -32601;
pub const JSONRPC_INVALID_PARAMS: i64 = -32602;
pub const JSONRPC_INTERNAL_ERROR: i64 = -32603;

/// MCP error code: requested tool does not exist.
pub const MCP_TOOL_NOT_FOUND: i64 = -32602;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    #[serde(default)]
    pub jsonrpc: Option<String>,
    pub id: Option<Value>,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl JsonRpcError {
    pub fn new(code: i64, message: impl Into<String>) -> Self {
        Self { code, message: message.into(), data: None }
    }
}

pub fn ok_response(id: Value, result: Value) -> JsonRpcResponse {
    JsonRpcResponse { jsonrpc: "2.0".into(), id, result: Some(result), error: None }
}

pub fn error_response(id: Value, code: i64, message: impl Into<String>) -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: "2.0".into(),
        id,
        result: None,
        error: Some(JsonRpcError::new(code, message)),
    }
}

/// A single MCP tool definition as exchanged in `tools/list` / `tools/call`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ToolDefinition {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default = "default_input_schema")]
    pub input_schema: Value,
}

fn default_input_schema() -> Value {
    json!({"type": "object", "properties": {}, "additionalProperties": false})
}

/// Build the MCP `initialize` result payload.
pub fn initialize_result() -> Value {
    json!({
        "protocolVersion": PROTOCOL_VERSION,
        "capabilities": { "tools": { "listChanged": false } },
        "serverInfo": { "name": SERVER_NAME, "version": env!("CARGO_PKG_VERSION") }
    })
}

/// Minimal Streamable-HTTP SSE body parser: tolerates both plain JSON responses
/// and `data: <json>` event-stream payloads.
pub fn parse_maybe_sse(body: &str) -> Value {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return Value::Null;
    }
    // plain JSON response
    if let Ok(v) = serde_json::from_str::<Value>(trimmed) {
        return v;
    }
    // SSE event stream: scan lines for `data: <json>`
    for line in trimmed.lines() {
        let line = line.trim();
        let payload = line.strip_prefix("data:").unwrap_or("").trim();
        if payload.is_empty() {
            continue;
        }
        if let Ok(v) = serde_json::from_str::<Value>(payload) {
            return v;
        }
    }
    Value::Null
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_definition_serializes_camel_case() {
        let t = ToolDefinition {
            name: "read_file".into(),
            description: Some("Read a file".into()),
            input_schema: json!({"type": "object"}),
        };
        let s = serde_json::to_string(&t).unwrap();
        assert!(s.contains("\"inputSchema\""));
        assert!(!s.contains("input_schema"));
    }

    #[test]
    fn sse_parser_extracts_data() {
        let body = "event: message\ndata: {\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{}}\n\n";
        let v = parse_maybe_sse(body);
        assert_eq!(v["id"], 1);
    }
}
