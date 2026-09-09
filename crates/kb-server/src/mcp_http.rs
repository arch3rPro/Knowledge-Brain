use std::{convert::Infallible, future::Future, sync::Arc};

use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, State, rejection::JsonRejection},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response, Sse, sse::Event},
    routing::post,
};
use kb_core::KbError;
use kb_mcp::McpServer;
use serde_json::{Value, json};
use tokio::net::TcpListener;

use crate::ServerPolicy;

const MAX_JSON_BODY_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone)]
pub struct McpHttpState {
    server: McpServer,
    policy: ServerPolicy,
    allowed_origins: Arc<[String]>,
}

impl McpHttpState {
    #[must_use]
    pub fn new(server: McpServer, policy: ServerPolicy, allowed_origins: Vec<String>) -> Self {
        Self {
            server,
            policy,
            allowed_origins: allowed_origins.into(),
        }
    }
}

/// Serve modern, stateless MCP over one Streamable HTTP endpoint.
///
/// # Errors
///
/// Returns an IO failure if the listener cannot serve requests.
pub async fn serve_mcp<F>(
    listener: TcpListener,
    state: McpHttpState,
    shutdown: F,
) -> Result<(), KbError>
where
    F: Future<Output = ()> + Send + 'static,
{
    let router = Router::new()
        .route("/mcp", post(mcp_post))
        .fallback(mcp_not_found)
        .method_not_allowed_fallback(mcp_method_not_allowed)
        .layer(DefaultBodyLimit::max(MAX_JSON_BODY_BYTES))
        .with_state(state);
    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown)
        .await
        .map_err(|error| KbError::io_failure("serve MCP HTTP", "listener", error.to_string()))
}

async fn mcp_post(
    State(state): State<McpHttpState>,
    headers: HeaderMap,
    request: Result<Json<Value>, JsonRejection>,
) -> Response {
    let id = request
        .as_ref()
        .ok()
        .and_then(|Json(value)| value.get("id"))
        .cloned()
        .unwrap_or(Value::Null);
    if !authorized(&state, &headers) {
        return rpc_response(
            StatusCode::UNAUTHORIZED,
            rpc_error(&id, -32001, "A valid Bearer token is required."),
        );
    }
    if !origin_allowed(&state, &headers) {
        return rpc_response(
            StatusCode::FORBIDDEN,
            rpc_error(&id, -32003, "Origin is not allowed."),
        );
    }
    if !accepts_streamable_http(&headers) {
        return rpc_response(
            StatusCode::NOT_ACCEPTABLE,
            rpc_error(
                &id,
                -32600,
                "Accept must include application/json and text/event-stream.",
            ),
        );
    }
    let Ok(Json(body)) = request else {
        return rpc_response(
            StatusCode::BAD_REQUEST,
            rpc_error(&id, -32700, "Request body must be valid JSON."),
        );
    };
    if let Some(response) = validate_headers(&headers, &body) {
        return rpc_response(StatusCode::BAD_REQUEST, response);
    }
    if body.get("id").is_none() {
        return StatusCode::ACCEPTED.into_response();
    }

    let method = body.get("method").and_then(Value::as_str);
    let server = state.server.clone();
    let request = body.clone();
    let response = match tokio::task::spawn_blocking(move || server.handle_modern(&request)).await {
        Ok(response) => response,
        Err(error) => {
            return rpc_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                rpc_error(
                    &id,
                    -32603,
                    &format!("MCP application task failed: {error}"),
                ),
            );
        }
    };
    let status = match response.pointer("/error/code").and_then(Value::as_i64) {
        Some(-32601) => StatusCode::NOT_FOUND,
        Some(_) => StatusCode::BAD_REQUEST,
        None => StatusCode::OK,
    };
    if method == Some("tools/call") && status == StatusCode::OK {
        let event = match Event::default().json_data(response) {
            Ok(event) => event,
            Err(error) => {
                return rpc_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    rpc_error(&id, -32603, &error.to_string()),
                );
            }
        };
        return Sse::new(tokio_stream::once(Ok::<_, Infallible>(event))).into_response();
    }
    rpc_response(status, response)
}

fn authorized(state: &McpHttpState, headers: &HeaderMap) -> bool {
    state.policy.authorize(
        headers
            .get(header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok()),
    )
}

fn origin_allowed(state: &McpHttpState, headers: &HeaderMap) -> bool {
    let Some(origin) = headers
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
    else {
        return true;
    };
    state
        .allowed_origins
        .iter()
        .any(|allowed| allowed == origin)
}

fn accepts_streamable_http(headers: &HeaderMap) -> bool {
    let Some(accept) = headers
        .get(header::ACCEPT)
        .and_then(|value| value.to_str().ok())
    else {
        return false;
    };
    let lower = accept.to_ascii_lowercase();
    lower.contains("application/json") && lower.contains("text/event-stream")
}

fn validate_headers(headers: &HeaderMap, body: &Value) -> Option<Value> {
    let id = body.get("id").unwrap_or(&Value::Null);
    let body_method = body.get("method").and_then(Value::as_str);
    let body_version = body
        .pointer("/params/_meta/io.modelcontextprotocol~1protocolVersion")
        .and_then(Value::as_str);
    let header_version = header_text(headers, "mcp-protocol-version");
    let header_method = header_text(headers, "mcp-method");
    if header_version != body_version || header_method != body_method {
        return Some(rpc_error(
            id,
            -32020,
            "MCP transport headers do not match the request body.",
        ));
    }
    if body_method == Some("tools/call") {
        let body_name = body.pointer("/params/name").and_then(Value::as_str);
        if header_text(headers, "mcp-name") != body_name {
            return Some(rpc_error(
                id,
                -32020,
                "Mcp-Name does not match the requested tool.",
            ));
        }
    }
    None
}

fn header_text<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers.get(name).and_then(|value| value.to_str().ok())
}

fn rpc_response(status: StatusCode, value: Value) -> Response {
    (status, Json(value)).into_response()
}

fn rpc_error(id: &Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc":"2.0", "id":id, "error":{"code":code,"message":message}})
}

async fn mcp_not_found(State(state): State<McpHttpState>, headers: HeaderMap) -> Response {
    if !authorized(&state, &headers) {
        return rpc_response(
            StatusCode::UNAUTHORIZED,
            rpc_error(&Value::Null, -32001, "A valid Bearer token is required."),
        );
    }
    rpc_response(
        StatusCode::NOT_FOUND,
        rpc_error(&Value::Null, -32601, "MCP endpoint not found."),
    )
}

async fn mcp_method_not_allowed(State(state): State<McpHttpState>, headers: HeaderMap) -> Response {
    if !authorized(&state, &headers) {
        return rpc_response(
            StatusCode::UNAUTHORIZED,
            rpc_error(&Value::Null, -32001, "A valid Bearer token is required."),
        );
    }
    rpc_response(
        StatusCode::METHOD_NOT_ALLOWED,
        rpc_error(&Value::Null, -32601, "Only POST /mcp is supported."),
    )
}
