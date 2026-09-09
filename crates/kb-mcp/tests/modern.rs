use kb_app::AppContext;
use kb_mcp::{MODERN_PROTOCOL_VERSION, McpServer};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::Path};

#[test]
fn discovery_reports_the_modern_stateless_contract() {
    let server = server();
    let response = server.handle_modern(&request(1, "server/discover", json!({})));

    assert_eq!(response["result"]["resultType"], "complete");
    assert_eq!(
        response["result"]["supportedVersions"],
        json!([MODERN_PROTOCOL_VERSION])
    );
    assert_eq!(response["result"]["capabilities"]["tools"], json!({}));
    assert_eq!(
        response["result"]["_meta"]["io.modelcontextprotocol/serverInfo"]["name"],
        "knowledge-brain"
    );
}

#[test]
fn modern_requests_require_metadata_and_reject_unknown_versions() {
    let server = server();
    let missing = server.handle_modern(&json!({
        "jsonrpc":"2.0", "id":1, "method":"tools/list", "params":{}
    }));
    assert_eq!(missing["error"]["code"], -32602);

    let unsupported = server.handle_modern(&json!({
        "jsonrpc":"2.0", "id":2, "method":"tools/list",
        "params":{"_meta": metadata("1900-01-01")}
    }));
    assert_eq!(unsupported["error"]["code"], -32022);
    assert_eq!(
        unsupported["error"]["data"]["supported"],
        json!([MODERN_PROTOCOL_VERSION])
    );
    assert_eq!(unsupported["error"]["data"]["requested"], "1900-01-01");
}

#[test]
fn modern_tools_are_cacheable_deterministic_and_return_complete_results() {
    let server = server();
    let listed = server.handle_modern(&request(1, "tools/list", json!({})));
    assert_eq!(listed["result"]["resultType"], "complete");
    assert_eq!(listed["result"]["ttlMs"], 300_000);
    assert_eq!(listed["result"]["cacheScope"], "private");
    let names = listed["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect::<Vec<_>>();
    let mut sorted = names.clone();
    sorted.sort_unstable();
    assert_eq!(names, sorted);

    let called = server.handle_modern(&request(
        2,
        "tools/call",
        json!({"name":"kb_capabilities","arguments":{}}),
    ));
    assert_eq!(called["result"]["resultType"], "complete");
    assert_eq!(called["result"]["isError"], false);
    assert_eq!(
        called["result"]["_meta"]["iose"],
        Value::Null,
        "server metadata must use the standard namespaced key"
    );
    assert!(called["result"]["_meta"]["io.modelcontextprotocol/serverInfo"].is_object());
}

#[test]
fn stdio_entry_point_routes_modern_requests_without_breaking_notifications() {
    let mut server = server();
    let response = server
        .handle(&request(7, "server/discover", json!({})))
        .unwrap();
    assert_eq!(response["result"]["resultType"], "complete");

    let notification = json!({
        "jsonrpc":"2.0",
        "method":"notifications/cancelled",
        "params":{"_meta": metadata(MODERN_PROTOCOL_VERSION)}
    });
    assert!(server.handle(&notification).is_none());
}

fn server() -> McpServer {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.keep();
    McpServer::new(context(&root), "unused-vault".into(), false)
}

fn request(id: u64, method: &str, mut params: Value) -> Value {
    params
        .as_object_mut()
        .unwrap()
        .insert("_meta".into(), metadata(MODERN_PROTOCOL_VERSION));
    json!({"jsonrpc":"2.0", "id":id, "method":method, "params":params})
}

fn metadata(version: &str) -> Value {
    json!({
        "io.modelcontextprotocol/protocolVersion": version,
        "io.modelcontextprotocol/clientInfo": {"name":"kb-test","version":"1"},
        "io.modelcontextprotocol/clientCapabilities": {}
    })
}

fn context(base: &Path) -> AppContext {
    let environment = BTreeMap::from([
        (
            "KB_CONFIG_DIR".into(),
            base.join("config").display().to_string(),
        ),
        (
            "KB_STATE_DIR".into(),
            base.join("state").display().to_string(),
        ),
        (
            "KB_CACHE_DIR".into(),
            base.join("cache").display().to_string(),
        ),
    ]);
    AppContext::new(environment, base.to_path_buf())
}
