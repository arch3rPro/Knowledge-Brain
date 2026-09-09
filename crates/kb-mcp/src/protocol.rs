use serde_json::{Map, Value, json};

use crate::McpServer;

pub const MODERN_PROTOCOL_VERSION: &str = "2026-07-28";
const CACHE_TTL_MS: u64 = 300_000;

pub(crate) fn handle_modern(server: &McpServer, request: &Value) -> Value {
    let Some(object) = request.as_object() else {
        return error(&Value::Null, -32600, "Invalid JSON-RPC request.", None);
    };
    let id = object.get("id").unwrap_or(&Value::Null);
    if object.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
        return error(id, -32600, "Invalid JSON-RPC request.", None);
    }
    let Some(method) = object.get("method").and_then(Value::as_str) else {
        return error(id, -32600, "Invalid JSON-RPC request.", None);
    };
    let Some(params) = object
        .get("params")
        .cloned()
        .unwrap_or_else(|| json!({}))
        .as_object()
        .cloned()
    else {
        return error(id, -32602, "Request params must be an object.", None);
    };
    let Some(metadata) = params.get("_meta").and_then(Value::as_object) else {
        return error(id, -32602, "Modern MCP request metadata is required.", None);
    };
    let Some(version) = metadata
        .get("io.modelcontextprotocol/protocolVersion")
        .and_then(Value::as_str)
    else {
        return error(
            id,
            -32602,
            "MCP protocol version metadata is required.",
            None,
        );
    };
    if version != MODERN_PROTOCOL_VERSION {
        return error(
            id,
            -32022,
            "Unsupported protocol version",
            Some(json!({
                "supported": [MODERN_PROTOCOL_VERSION],
                "requested": version
            })),
        );
    }
    if !metadata
        .get("io.modelcontextprotocol/clientCapabilities")
        .is_some_and(Value::is_object)
    {
        return error(
            id,
            -32602,
            "MCP client capabilities metadata is required.",
            None,
        );
    }

    match method {
        "server/discover" => complete(
            id,
            json!({
                "supportedVersions": [MODERN_PROTOCOL_VERSION],
                "capabilities": { "tools": {} },
                "ttlMs": CACHE_TTL_MS,
                "cacheScope": "private"
            }),
        ),
        "tools/list" => {
            let mut tools = server.tools();
            tools.sort_by(|left, right| left["name"].as_str().cmp(&right["name"].as_str()));
            complete(
                id,
                json!({
                    "tools": tools,
                    "ttlMs": CACHE_TTL_MS,
                    "cacheScope": "private"
                }),
            )
        }
        "tools/call" => {
            let mut call_params = params;
            call_params.remove("_meta");
            let legacy = server.call_tool(id, Value::Object(call_params));
            match legacy.get("result").cloned() {
                Some(result) => complete(id, result),
                None => legacy,
            }
        }
        _ => error(id, -32601, "Method not found.", None),
    }
}

fn complete(id: &Value, result: Value) -> Value {
    let mut result = match result {
        Value::Object(object) => object,
        value => {
            let mut object = Map::new();
            object.insert("value".into(), value);
            object
        }
    };
    result.insert("resultType".into(), Value::String("complete".into()));
    result.insert("_meta".into(), server_metadata());
    json!({"jsonrpc":"2.0", "id":id, "result":result})
}

fn server_metadata() -> Value {
    json!({
        "io.modelcontextprotocol/serverInfo": {
            "name": "knowledge-brain",
            "version": env!("CARGO_PKG_VERSION")
        }
    })
}

pub(crate) fn error(id: &Value, code: i64, message: &str, data: Option<Value>) -> Value {
    let mut error = json!({"code":code,"message":message});
    if let Some(data) = data {
        error
            .as_object_mut()
            .expect("error is an object")
            .insert("data".into(), data);
    }
    json!({"jsonrpc":"2.0", "id":id, "error":error})
}
