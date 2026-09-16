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

const OLD_KB: &str = include_str!("../../../assets/vault-template-history/v1.1/KB.md");
const OLD_MANIFEST: &str = include_str!("../../../assets/vault-template-history/v1.1/template.yml");

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
    .with_update_runtime(kb_app::UpdateRuntime {
        identity: kb_update::BuildIdentity::development(env!("CARGO_PKG_VERSION")).unwrap(),
        executable: std::env::current_exe().unwrap(),
        executable_managed: false,
    })
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
    transport_request(
        server,
        method,
        &format!(
            "MCP-Protocol-Version: {MODERN_PROTOCOL_VERSION}\r\nMcp-Method: {}\r\n{extra_headers}",
            body["method"].as_str().unwrap_or("none"),
        ),
        body,
    )
    .await
}

async fn transport_request(
    server: &RunningServer,
    method: &str,
    headers: &str,
    body: &Value,
) -> (u16, String, String) {
    let encoded = serde_json::to_string(body).unwrap();
    let mut stream = TcpStream::connect(server.address).await.unwrap();
    let wire = format!(
        "{method} /mcp HTTP/1.1\r\nHost: {}\r\nContent-Type: application/json\r\nAccept: application/json, text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n{headers}\r\n{encoded}",
        server.address,
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
async fn initialize_based_clients_can_list_and_call_tools_over_http() {
    let server = start(None, false, vec![]).await;
    let initialize = json!({
        "jsonrpc":"2.0",
        "id":1,
        "method":"initialize",
        "params":{
            "protocolVersion":"2025-11-25",
            "capabilities":{},
            "clientInfo":{"name":"compatibility-test","version":"1"}
        }
    });
    let (status, _, response) = transport_request(&server, "POST", "", &initialize).await;
    assert_eq!(status, 200, "{response}");
    assert_eq!(
        serde_json::from_str::<Value>(&response).unwrap()["result"]["protocolVersion"],
        "2025-11-25"
    );

    let initialized = json!({
        "jsonrpc":"2.0",
        "method":"notifications/initialized",
        "params":{}
    });
    let (status, _, _) = transport_request(
        &server,
        "POST",
        "MCP-Protocol-Version: 2025-11-25\r\n",
        &initialized,
    )
    .await;
    assert_eq!(status, 202);

    let list = json!({"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}});
    let (status, _, response) = transport_request(
        &server,
        "POST",
        "MCP-Protocol-Version: 2025-11-25\r\n",
        &list,
    )
    .await;
    assert_eq!(status, 200, "{response}");
    assert!(
        serde_json::from_str::<Value>(&response).unwrap()["result"]["tools"]
            .as_array()
            .is_some_and(|tools| !tools.is_empty())
    );

    let call = json!({
        "jsonrpc":"2.0",
        "id":3,
        "method":"tools/call",
        "params":{"name":"kb_capabilities","arguments":{}}
    });
    let (status, headers, response) = transport_request(
        &server,
        "POST",
        "MCP-Protocol-Version: 2025-11-25\r\n",
        &call,
    )
    .await;
    assert_eq!(status, 200, "{response}");
    assert!(headers.to_ascii_lowercase().contains("text/event-stream"));
    assert_eq!(sse_json(&response)["result"]["isError"], false);

    let vault_id = std::fs::read_to_string(server.vault.join(".kb/config.yml"))
        .unwrap()
        .lines()
        .find_map(|line| line.strip_prefix("vault_id: "))
        .unwrap()
        .trim_matches('"')
        .to_owned();
    let read = json!({
        "jsonrpc":"2.0",
        "id":4,
        "method":"resources/read",
        "params":{"uri":format!("kb-vault://{vault_id}/KB.md")}
    });
    let (status, _, response) = transport_request(
        &server,
        "POST",
        "MCP-Protocol-Version: 2025-11-25\r\n",
        &read,
    )
    .await;
    assert_eq!(status, 200, "{response}");
    let response = serde_json::from_str::<Value>(&response).unwrap();
    assert!(
        response["result"]["contents"][0]["text"]
            .as_str()
            .unwrap()
            .contains("Knowledge-Brain")
    );
}

#[tokio::test]
async fn initialize_negotiates_each_supported_legacy_version() {
    let server = start(None, false, vec![]).await;
    for (id, version) in ["2025-11-25", "2025-06-18", "2025-03-26"]
        .into_iter()
        .enumerate()
    {
        let initialize = json!({
            "jsonrpc":"2.0",
            "id":id,
            "method":"initialize",
            "params":{
                "protocolVersion":version,
                "capabilities":{},
                "clientInfo":{"name":"compatibility-test","version":"1"}
            }
        });
        let (status, _, response) = transport_request(&server, "POST", "", &initialize).await;
        assert_eq!(status, 200, "{response}");
        assert_eq!(
            serde_json::from_str::<Value>(&response).unwrap()["result"]["protocolVersion"],
            version
        );
    }
}

#[tokio::test]
async fn unknown_versions_are_rejected_instead_of_falling_back_to_legacy() {
    let server = start(None, false, vec![]).await;
    let list = json!({"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}});
    let (status, _, response) = transport_request(
        &server,
        "POST",
        "MCP-Protocol-Version: 1900-01-01\r\n",
        &list,
    )
    .await;
    assert_eq!(status, 400);
    let error = serde_json::from_str::<Value>(&response).unwrap();
    assert_eq!(error["error"]["code"], -32022);
    assert_eq!(error["error"]["data"]["requested"], "1900-01-01");

    let metadata_only = json!({
        "jsonrpc":"2.0",
        "id":2,
        "method":"tools/list",
        "params":{"_meta":{
            "io.modelcontextprotocol/protocolVersion":"1900-01-01",
            "io.modelcontextprotocol/clientCapabilities":{}
        }}
    });
    let (status, _, response) = transport_request(&server, "POST", "", &metadata_only).await;
    assert_eq!(status, 400);
    let error = serde_json::from_str::<Value>(&response).unwrap();
    assert_eq!(error["error"]["code"], -32022);
    assert_eq!(error["error"]["data"]["requested"], "1900-01-01");
}

#[tokio::test]
async fn modern_version_cannot_be_combined_with_initialize() {
    let server = start(None, false, vec![]).await;
    let initialize = json!({
        "jsonrpc":"2.0",
        "id":1,
        "method":"initialize",
        "params":{
            "protocolVersion":"2025-11-25",
            "capabilities":{},
            "clientInfo":{"name":"compatibility-test","version":"1"}
        }
    });
    let (status, _, response) = transport_request(
        &server,
        "POST",
        &format!("MCP-Protocol-Version: {MODERN_PROTOCOL_VERSION}\r\n"),
        &initialize,
    )
    .await;
    assert_eq!(status, 400);
    assert_eq!(
        serde_json::from_str::<Value>(&response).unwrap()["error"]["code"],
        -32022
    );
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

#[tokio::test]
async fn update_plan_and_confirmation_work_over_streamable_http() {
    let server = start(Some("secret"), true, vec![]).await;
    std::fs::write(server.vault.join("KB.md"), OLD_KB).unwrap();
    std::fs::write(server.vault.join(".kb/template.yml"), OLD_MANIFEST).unwrap();
    let plan = body(
        1,
        "tools/call",
        json!({"name":"kb_update_plan","arguments":{}}),
    );
    let (status, _, response) = request(
        &server,
        "POST",
        "Authorization: Bearer secret\r\nMcp-Name: kb_update_plan\r\n",
        &plan,
    )
    .await;
    assert_eq!(status, 200);
    let planned = sse_json(&response);
    let token = planned["result"]["structuredContent"]["data"]["confirmation_token"]
        .as_str()
        .unwrap();

    let confirm = body(
        2,
        "tools/call",
        json!({
            "name":"kb_update_confirm",
            "arguments":{"confirmation_token":token}
        }),
    );
    let (status, _, response) = request(
        &server,
        "POST",
        "Authorization: Bearer secret\r\nMcp-Name: kb_update_confirm\r\n",
        &confirm,
    )
    .await;
    assert_eq!(status, 200);
    let applied = sse_json(&response);
    assert_eq!(applied["result"]["isError"], false);
    assert_eq!(
        applied["result"]["structuredContent"]["data"]["execution_state"],
        "completed_with_skips"
    );
    assert!(
        std::fs::read_to_string(server.vault.join(".kb/template.yml"))
            .unwrap()
            .contains("template_version: v1.3")
    );
}

fn sse_json(response: &str) -> Value {
    let data = response
        .lines()
        .find_map(|line| line.strip_prefix("data: "))
        .unwrap();
    serde_json::from_str(data).unwrap()
}
