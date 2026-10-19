//! Streamable HTTP MCP server endpoint (`POST /mcp`) that Codex connects to.
//!
//! Session handling is deliberately light: every request is serviced from the
//! shared gateway (cache for `tools/list`, live forwarding for `tools/call`),
//! and a `Mcp-Session-Id` header is issued on `initialize` for compatibility
//! with strict clients, but not enforced on subsequent requests.

use std::sync::Arc;

use axum::body::Body;
use axum::extract::{OriginalUri, State};
use axum::http::{header, HeaderMap, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::Router;
use serde_json::{json, Value};

use crate::gateway::GatewayHandle;
use crate::protocol::{
    error_response, initialize_result, ok_response, JsonRpcRequest, JsonRpcResponse, JSONRPC_INVALID_REQUEST,
    JSONRPC_METHOD_NOT_FOUND, MCP_TOOL_NOT_FOUND,
};

type SharedGateway = Arc<GatewayHandle>;

/// Start the gateway HTTP server on 127.0.0.1:port. Returns the bind error if
/// the port is already taken, otherwise a join handle for the server task.
pub async fn spawn_server(gateway: SharedGateway, port: u16) -> anyhow::Result<tokio::task::JoinHandle<()>> {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port)).await?;
    let bound_port = listener.local_addr()?.port();
    let app = Router::new()
        .route("/mcp", post(handle_post))
        .with_state(gateway)
        .layer(axum::middleware::from_fn(
            move |request: axum::extract::Request, next: axum::middleware::Next| async move {
                if !local_request_allowed(request.headers(), bound_port) {
                    return StatusCode::FORBIDDEN.into_response();
                }
                next.run(request).await
            },
        ));
    log::info!("MCP gateway listening on http://127.0.0.1:{port}/mcp");
    let handle = tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, app).await {
            log::error!("MCP gateway server exited: {e}");
        }
    });
    Ok(handle)
}

// Native MCP clients do not send Origin. Browser requests must originate from
// this loopback endpoint; arbitrary Host values are rejected against DNS rebinding.
fn local_request_allowed(headers: &HeaderMap, port: u16) -> bool {
    let hosts = [format!("127.0.0.1:{port}"), format!("localhost:{port}")];
    let Some(host) = headers.get(header::HOST).and_then(|value| value.to_str().ok()) else {
        return false;
    };
    if !hosts.iter().any(|allowed| host.eq_ignore_ascii_case(allowed)) {
        return false;
    }
    match headers.get(header::ORIGIN) {
        None => true,
        Some(origin) => origin.to_str().ok().is_some_and(|origin| {
            hosts
                .iter()
                .any(|host| origin.eq_ignore_ascii_case(&format!("http://{host}")))
        }),
    }
}

#[cfg(test)]
mod security_tests {
    use super::*;

    #[test]
    fn native_loopback_clients_remain_supported() {
        let mut headers = HeaderMap::new();
        headers.insert(header::HOST, "127.0.0.1:9876".parse().unwrap());
        assert!(local_request_allowed(&headers, 9876));
        headers.insert(header::ORIGIN, "http://localhost:9876".parse().unwrap());
        assert!(local_request_allowed(&headers, 9876));
    }

    #[test]
    fn reject_web_origins_rebinding_hosts_and_wrong_ports() {
        let mut headers = HeaderMap::new();
        headers.insert(header::HOST, "127.0.0.1:9876".parse().unwrap());
        for origin in ["https://evil.example", "null", "http://localhost:9877"] {
            headers.insert(header::ORIGIN, origin.parse().unwrap());
            assert!(!local_request_allowed(&headers, 9876));
        }
        headers.remove(header::ORIGIN);
        headers.insert(header::HOST, "evil.example:9876".parse().unwrap());
        assert!(!local_request_allowed(&headers, 9876));
        headers.remove(header::HOST);
        assert!(!local_request_allowed(&headers, 9876));
    }
}

/// Convenience: load config, start the server, return port actually bound.
/// (Auto warm-start is spawned by the host — see src-tauri setup — using
/// `GatewayHandle::run_warm_start` on the host's own async runtime.)
/// Used from the Tauri setup path.
pub async fn start_gateway(
    config_path: &std::path::Path,
    cache_path: &std::path::Path,
) -> anyhow::Result<(GatewayHandle, u16, tokio::task::JoinHandle<()>)> {
    let gateway = GatewayHandle::load(config_path, cache_path)?;
    let port = gateway.port().await;
    let handle = spawn_server(Arc::new(gateway.clone()), port).await?;
    Ok((gateway, port, handle))
}

/// Parse the `?scope=<server>` query parameter. When present, `tools/list`
/// only exposes that one server's tools and `tools/call` rejects foreign
/// tools, letting Codex expose each external MCP as its own entry.
fn scope_from_uri(uri: &Uri) -> Option<String> {
    uri.query().and_then(|q| {
        q.split('&').find_map(|kv| {
            let mut it = kv.splitn(2, '=');
            match (it.next(), it.next()) {
                (Some("scope"), Some(v)) if !v.is_empty() => Some(v.to_string()),
                _ => None,
            }
        })
    })
}

async fn handle_post(
    State(gateway): State<SharedGateway>,
    headers: HeaderMap,
    uri: OriginalUri,
    body: axum::body::Bytes,
) -> Response {
    let scope = scope_from_uri(&uri);
    let req: JsonRpcRequest = match serde_json::from_slice(&body) {
        Ok(r) => r,
        Err(_) => {
            return json_response(
                headers,
                error_response(Value::Null, JSONRPC_INVALID_REQUEST, "invalid JSON-RPC request"),
                None,
            );
        }
    };

    // Streamable HTTP acknowledges notifications with HTTP 202 and no body.
    // A JSON-RPC response with id:null and neither result nor error is invalid
    // and causes Codex's transport to disconnect during initialization.
    if req.id.is_none() {
        return StatusCode::ACCEPTED.into_response();
    }
    let id = req.id.clone().unwrap_or(Value::Null);

    let (session_header, resp): (Option<String>, JsonRpcResponse) = match req.method.as_str() {
        "initialize" => {
            let session = uuid::Uuid::new_v4().to_string();
            (Some(session), ok_response(id, initialize_result()))
        }
        "ping" => (None, ok_response(id, json!({}))),
        "tools/list" => {
            let tool_list: Vec<Value> = if gateway.meta_mode().await {
                // Meta-mode: expose the compact gateway_* surface instead of the
                // full backend catalog, so context stays small.
                gateway
                    .meta_tools()
                    .await
                    .into_iter()
                    .map(|t| {
                        json!({
                            "name": t.name,
                            "description": t.description,
                            "inputSchema": t.input_schema,
                        })
                    })
                    .collect()
            } else if let Some(scope) = &scope {
                // Scope mode: only this server's tools (per-MCP entries).
                gateway
                    .list_tools_for(scope)
                    .await
                    .into_iter()
                    .map(|a| {
                        json!({
                            "name": a.qualified_name,
                            "description": a.tool.description,
                            "inputSchema": a.tool.input_schema,
                        })
                    })
                    .collect()
            } else {
                gateway
                    .list_all_tools()
                    .await
                    .into_iter()
                    .map(|a| {
                        json!({
                            "name": a.qualified_name,
                            "description": a.tool.description,
                            "inputSchema": a.tool.input_schema,
                        })
                    })
                    .collect()
            };
            (None, ok_response(id, json!({"tools": tool_list})))
        }
        "tools/call" => {
            let name = req.params.get("name").and_then(Value::as_str).unwrap_or("");
            let arguments = req.params.get("arguments").cloned().unwrap_or_else(|| json!({}));
            if let Some(scope) = &scope {
                // A scoped entry may only call its own group's tools (meta
                // tools are exempt so meta mode stays usable behind a scope).
                if !name.starts_with("gateway_") {
                    let allowed = gateway.list_tools_for(scope).await;
                    let known = allowed.iter().any(|a| a.qualified_name == name);
                    if !known {
                        return json_response(
                            headers,
                            error_response(
                                id,
                                MCP_TOOL_NOT_FOUND,
                                format!("tool `{name}` is not part of scope `{scope}`"),
                            ),
                            None,
                        );
                    }
                }
            }
            let outcome = if name.starts_with("gateway_") {
                gateway.handle_meta_call(name, arguments).await
            } else {
                gateway.call_tool(name, arguments).await
            };
            match outcome {
                Ok(result) => {
                    let payload = if result.get("content").is_some() || result.get("structuredContent").is_some() {
                        result
                    } else {
                        // wrap raw values into MCP text content
                        json!({"content": [{"type": "text", "text": result.to_string()}]})
                    };
                    (None, ok_response(id, payload))
                }
                Err(e) => {
                    let msg = e.to_string();
                    (
                        None,
                        JsonRpcResponse {
                            jsonrpc: "2.0".into(),
                            id,
                            result: Some(json!({
                                "content": [{"type": "text", "text": format!("MCP call failed: {msg}")}],
                                "isError": true
                            })),
                            error: None,
                        },
                    )
                }
            }
        }
        other => (
            None,
            error_response(id, JSONRPC_METHOD_NOT_FOUND, format!("method not found: {other}")),
        ),
    };

    // MCP_TOOL_NOT_FOUND unused here but kept for API symmetry
    let _ = MCP_TOOL_NOT_FOUND;

    json_response(headers, resp, session_header)
}

fn json_response(headers: HeaderMap, resp: JsonRpcResponse, session: Option<String>) -> Response {
    let body = match serde_json::to_string(&resp) {
        Ok(s) => s,
        Err(_) => "{}".to_string(),
    };
    let mut builder = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::CACHE_CONTROL, "no-store");
    if let Some(sid) = session {
        builder = builder.header("Mcp-Session-Id", sid);
    }
    // tolerate clients that ignore the session header
    let _ = headers;
    builder.body(Body::from(body)).unwrap_or_else(|_| {
        Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(Body::from("internal error"))
            .unwrap()
    })
}

#[allow(dead_code)]
fn sse_response(_session: Option<String>) -> Response {
    // SSE streaming is not implemented in v1; plain JSON responses are spec-compliant
    // for non-streaming clients.
    unreachable!()
}
