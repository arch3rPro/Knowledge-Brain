use std::{
    collections::BTreeMap,
    future::pending,
    net::SocketAddr,
    path::{Path, PathBuf},
};

use kb_app::{AppContext, AppRequest, InitRequest};
use kb_mcp::{MODERN_PROTOCOL_VERSION, McpServer};
use kb_server::{McpHttpState, ServerPolicy, serve_mcp};
use serde_json::{Value, json};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    task::JoinHandle,
};

struct RunningServer {
    address: SocketAddr,
    task: JoinHandle<()>,
    vault: PathBuf,
    _temporary: tempfile::TempDir,
}

impl Drop for RunningServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}

fn context(base: &Path) -> AppContext {
    AppContext::new(
        BTreeMap::from([
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
        ]),
        base.to_path_buf(),
    )
}

async fn start(
    token: Option<&str>,
    allow_write: bool,
    allowed_origins: Vec<String>,
) -> RunningServer {
    let temporary = tempfile::tempdir().unwrap();
    let context = context(temporary.path());
    let vault = temporary.path().join("vault");
    kb_app::run(
        AppRequest::Init(InitRequest {
            target: vault.clone(),
        }),
        &context,
    )
    .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let policy = ServerPolicy::new(address, token.map(str::to_owned), allow_write).unwrap();
    let mcp = McpServer::new(context, vault.display().to_string(), allow_write);
    let state = McpHttpState::new(mcp, policy, allowed_origins);
    let task = tokio::spawn(async move { serve_mcp(listener, state, pending()).await.unwrap() });
    RunningServer {
        address,
        task,
        vault,
        _temporary: temporary,
    }
}

fn body(id: u64, method: &str, params: Value) -> Value {
    let Value::Object(mut params) = params else {
        panic!("test params must be an object");
    };
    params.insert(
        "_meta".into(),
        json!({
            "io.modelcontextprotocol/protocolVersion": MODERN_PROTOCOL_VERSION,
            "io.modelcontextprotocol/clientCapabilities": {}
        }),
    );
    json!({"jsonrpc":"2.0","id":id,"method":method,"params":params})
}

async fn request(
    server: &RunningServer,
    method: &str,
    extra_headers: &str,
    body: &Value,
) -> (u16, String, String) {
    let encoded = serde_json::to_string(body).unwrap();
    let mut stream = TcpStream::connect(server.address).await.unwrap();
    let wire = format!(
        "{method} /mcp HTTP/1.1\r\nHost: {}\r\nContent-Type: application/json\r\nAccept: application/json, text/event-stream\r\nMCP-Protocol-Version: {MODERN_PROTOCOL_VERSION}\r\nMcp-Method: {}\r\nContent-Length: {}\r\nConnection: close\r\n{extra_headers}\r\n{encoded}",
        server.address,
        body["method"].as_str().unwrap_or("none"),
        encoded.len(),
    );
    stream.write_all(wire.as_bytes()).await.unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).await.unwrap();
    let (headers, response_body) = response.split_once("\r\n\r\n").unwrap();
    let status = headers.split_whitespace().nth(1).unwrap().parse().unwrap();
    (status, headers.to_owned(), response_body.to_owned())
}

#[tokio::test]
async fn discovery_is_json_and_tool_calls_use_request_scoped_sse() {
    let server = start(None, false, vec![]).await;
    let discover = body(1, "server/discover", json!({}));
    let (status, headers, response) = request(&server, "POST", "", &discover).await;
    assert_eq!(status, 200);
    assert!(headers.to_ascii_lowercase().contains("application/json"));
    assert_eq!(
        serde_json::from_str::<Value>(&response).unwrap()["result"]["resultType"],
        "complete"
    );

    let call = body(
        2,
        "tools/call",
        json!({"name":"kb_capabilities","arguments":{}}),
    );
    let (status, headers, response) =
        request(&server, "POST", "Mcp-Name: kb_capabilities\r\n", &call).await;
    assert_eq!(status, 200);
    assert!(headers.to_ascii_lowercase().contains("text/event-stream"));
    assert!(
        response.contains("\"resultType\":\"complete\""),
        "{response}"
    );
}

#[tokio::test]
async fn rejects_bad_origin_auth_and_transport_header_mismatch() {
    let server = start(
        Some("secret"),
        false,
        vec!["https://allowed.example".into()],
    )
    .await;
    let discover = body(1, "server/discover", json!({}));

    let (status, _, _) = request(&server, "POST", "", &discover).await;
    assert_eq!(status, 401);

    let (status, _, _) = request(
        &server,
        "POST",
        "Authorization: Bearer secret\r\nOrigin: https://evil.example\r\n",
        &discover,
    )
    .await;
    assert_eq!(status, 403);

    let mismatched = json!({
        "jsonrpc":"2.0","id":1,"method":"server/discover","params":{"_meta":{
            "io.modelcontextprotocol/protocolVersion":"1900-01-01",
            "io.modelcontextprotocol/clientCapabilities":{}
        }}
    });
    let (status, _, response) = request(
        &server,
        "POST",
        "Authorization: Bearer secret\r\nOrigin: https://allowed.example\r\n",
        &mismatched,
    )
    .await;
    assert_eq!(status, 400);
    assert_eq!(
        serde_json::from_str::<Value>(&response).unwrap()["error"]["code"],
        -32020
    );
}

#[tokio::test]
async fn authenticated_write_survives_the_http_request() {
    let server = start(Some("secret"), true, vec![]).await;
    let plan = body(
        1,
        "tools/call",
        json!({
            "name":"kb_plan_knowledge",
            "arguments":{"request":{"schema_version":"v1.0","changes":[{
                "path":"articles/http-mcp.md",
                "before_sha256":null,
                "summary":"Exercise modern MCP HTTP write",
                "content":"---\ntype: Article\ntitle: HTTP MCP\nstatus: stable\ngenerated:\n  by: process:mcp-http-test\n  at: 2026-09-09T03:00:00Z\nsources:\n  - id: mcp\n    resource: https://modelcontextprotocol.io/\nkb:\n  managed: true\n---\n\n# HTTP MCP\n\nmodern-http-persisted\n"
            }]}}
        }),
    );
    let (_, _, response) = request(
        &server,
        "POST",
        "Authorization: Bearer secret\r\nMcp-Name: kb_plan_knowledge\r\n",
        &plan,
    )
    .await;
    let planned = sse_json(&response);
    let operation_id = planned["result"]["structuredContent"]["data"]["operation_id"]
        .as_str()
        .unwrap();

    let apply = body(
        2,
        "tools/call",
        json!({"name":"kb_apply_operation","arguments":{"operation_id":operation_id}}),
    );
    let (status, _, response) = request(
        &server,
        "POST",
        "Authorization: Bearer secret\r\nMcp-Name: kb_apply_operation\r\n",
        &apply,
    )
    .await;
    assert_eq!(status, 200);
    let applied = sse_json(&response);
    assert_eq!(applied["result"]["isError"], false);
    let saved = server.vault.join("Wiki/articles/http-mcp.md");
    assert!(saved.is_file());
    assert!(
        std::fs::read_to_string(saved)
            .unwrap()
            .contains("modern-http-persisted")
    );
}

fn sse_json(response: &str) -> Value {
    let data = response
        .lines()
        .find_map(|line| line.strip_prefix("data: "))
        .unwrap();
    serde_json::from_str(data).unwrap()
}
