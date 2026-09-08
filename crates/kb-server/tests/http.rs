use std::{collections::BTreeMap, future::pending, net::SocketAddr, path::Path};

use kb_app::{AppContext, AppRequest, InitRequest};
use kb_server::{ServerPolicy, ServerState, serve};
use serde_json::{Value, json};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    task::JoinHandle,
};

struct RunningServer {
    address: SocketAddr,
    task: JoinHandle<()>,
}

impl Drop for RunningServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}

fn context(base: &Path) -> AppContext {
    let environment = BTreeMap::from([
        (
            "KB_CONFIG_DIR".to_owned(),
            base.join("config").display().to_string(),
        ),
        (
            "KB_STATE_DIR".to_owned(),
            base.join("state").display().to_string(),
        ),
        (
            "KB_CACHE_DIR".to_owned(),
            base.join("cache").display().to_string(),
        ),
    ]);
    AppContext::new(environment, base.to_path_buf())
}

fn initialize(context: &AppContext, vault: &Path) -> Value {
    kb_app::run(
        AppRequest::Init(InitRequest {
            target: vault.to_path_buf(),
        }),
        context,
    )
    .unwrap()
}

async fn start(
    context: AppContext,
    vault: &Path,
    token: Option<&str>,
    allow_write: bool,
) -> RunningServer {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let policy = ServerPolicy::new(address, token.map(str::to_owned), allow_write).unwrap();
    let state = ServerState::new(context, vault.display().to_string(), policy);
    let task = tokio::spawn(async move {
        serve(listener, state, pending()).await.unwrap();
    });
    RunningServer { address, task }
}

async fn request(
    server: &RunningServer,
    method: &str,
    path: &str,
    token: Option<&str>,
    body: &str,
) -> (u16, Value) {
    let mut stream = TcpStream::connect(server.address).await.unwrap();
    let authorization = token.map_or_else(String::new, |value| {
        format!("Authorization: Bearer {value}\r\n")
    });
    let wire = format!(
        "{method} {path} HTTP/1.1\r\nHost: {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n{authorization}\r\n{body}",
        server.address,
        body.len(),
    );
    stream.write_all(wire.as_bytes()).await.unwrap();
    let mut response = Vec::new();
    stream.read_to_end(&mut response).await.unwrap();
    let response = String::from_utf8(response).unwrap();
    let (headers, body) = response.split_once("\r\n\r\n").unwrap();
    let status = headers
        .split_whitespace()
        .nth(1)
        .unwrap()
        .parse::<u16>()
        .unwrap();
    (status, serde_json::from_str(body).unwrap())
}

async fn event_stream(
    server: &RunningServer,
    path: &str,
    token: Option<&str>,
    cursor: Option<u64>,
) -> String {
    let mut stream = TcpStream::connect(server.address).await.unwrap();
    let authorization = token.map_or_else(String::new, |value| {
        format!("Authorization: Bearer {value}\r\n")
    });
    let last_event = cursor.map_or_else(String::new, |value| format!("Last-Event-ID: {value}\r\n"));
    let wire = format!(
        "GET {path} HTTP/1.1\r\nHost: {}\r\nAccept: text/event-stream\r\nConnection: close\r\n{authorization}{last_event}\r\n",
        server.address,
    );
    stream.write_all(wire.as_bytes()).await.unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).await.unwrap();
    response
}

fn knowledge_request() -> Value {
    json!({
        "schema_version": "v1.0",
        "changes": [{
            "path": "articles/http.md",
            "before_sha256": null,
            "summary": "Add the HTTP adapter note.",
            "content": "---\ntype: Article\ntitle: HTTP adapter\nstatus: stable\ngenerated:\n  by: process:http-test\n  at: 2026-09-07T03:00:00Z\nsources:\n  - id: source\n    resource: https://example.com/http\nkb:\n  managed: true\n---\n\n# HTTP adapter\n"
        }]
    })
}

#[tokio::test]
async fn read_routes_use_the_shared_envelope_and_machine_errors() {
    let temporary = tempfile::tempdir().unwrap();
    let context = context(temporary.path());
    let vault = temporary.path().join("vault");
    initialize(&context, &vault);
    let server = start(context, &vault, None, false).await;

    let (status, response) = request(&server, "GET", "/status", None, "").await;
    assert_eq!(status, 200);
    assert_eq!(response["schema_version"], "v1.0");
    assert_eq!(response["data"]["root"], vault.display().to_string());

    let (status, response) = request(&server, "POST", "/query", None, "{").await;
    assert_eq!(status, 400);
    assert_eq!(response["error"]["code"], "invalid_config");

    let (status, response) = request(&server, "GET", "/missing", None, "").await;
    assert_eq!(status, 404);
    assert_eq!(response["error"]["code"], "capability_unavailable");

    let (status, response) = request(&server, "GET", "/query", None, "").await;
    assert_eq!(status, 405);
    assert_eq!(response["error"]["code"], "capability_unavailable");

    let oversized = format!("{{\"query\":\"{}\"}}", "x".repeat(1024 * 1024));
    let (status, response) = request(&server, "POST", "/query", None, &oversized).await;
    assert_eq!(status, 400);
    assert_eq!(response["error"]["code"], "invalid_config");
}

#[tokio::test]
async fn token_protects_every_route_and_read_only_denies_apply() {
    let temporary = tempfile::tempdir().unwrap();
    let context = context(temporary.path());
    let vault = temporary.path().join("vault");
    initialize(&context, &vault);
    let plan = kb_app::run(
        AppRequest::PlanCreate {
            vault: Some(vault.display().to_string()),
            request: serde_json::from_value(knowledge_request()).unwrap(),
        },
        &context,
    )
    .unwrap();
    let operation_id = plan["operation_id"].as_str().unwrap();
    let server = start(context, &vault, Some("secret"), false).await;

    let (status, response) = request(&server, "GET", "/status", None, "").await;
    assert_eq!(status, 401);
    assert_eq!(response["error"]["code"], "auth_denied");

    let events_path = format!("/operations/{operation_id}/events");
    let (status, response) = request(&server, "GET", &events_path, None, "").await;
    assert_eq!(status, 401);
    assert_eq!(response["error"]["code"], "auth_denied");

    let path = format!("/operations/{operation_id}/apply");
    let (status, response) = request(&server, "POST", &path, Some("secret"), "").await;
    assert_eq!(status, 403);
    assert_eq!(response["error"]["code"], "auth_denied");
    assert!(!vault.join("Wiki/articles/http.md").exists());
}

#[tokio::test]
async fn enabled_apply_uses_a_reviewed_plan_for_the_fixed_vault() {
    let temporary = tempfile::tempdir().unwrap();
    let context = context(temporary.path());
    let vault = temporary.path().join("vault");
    initialize(&context, &vault);
    let server = start(context, &vault, Some("secret"), true).await;

    let body = serde_json::to_string(&knowledge_request()).unwrap();
    let (status, response) = request(&server, "POST", "/plans", Some("secret"), &body).await;
    assert_eq!(status, 200, "{response}");
    let operation_id = response["data"]["operation_id"]
        .as_str()
        .unwrap()
        .to_owned();

    let inspect_path = format!("/operations/{operation_id}");
    let (status, response) = request(&server, "GET", &inspect_path, Some("secret"), "").await;
    assert_eq!(status, 200);
    assert_eq!(response["data"]["state"], "planned");
    assert!(response["data"]["plan"].is_object());
    assert_eq!(
        response["data"]["operation_summary"]["requires_confirmation"],
        true
    );
    assert_eq!(
        response["data"]["operation_summary"]["operation_id"],
        operation_id
    );

    let apply_path = format!("/operations/{operation_id}/apply");
    let (status, response) = request(&server, "POST", &apply_path, Some("secret"), "").await;
    assert_eq!(status, 200, "{response}");
    assert_eq!(response["data"]["operation_id"], operation_id);
    assert!(vault.join("Wiki/articles/http.md").is_file());

    let (status, response) = request(&server, "GET", &inspect_path, Some("secret"), "").await;
    assert_eq!(status, 200);
    assert_eq!(response["data"]["state"], "applied");
    assert!(response["data"]["result"].is_object());
    assert_eq!(
        response["data"]["operation_summary"]["operation_id"],
        operation_id
    );
    assert_eq!(response["data"]["operation_summary"]["can_apply"], false);
}

#[tokio::test]
async fn operation_ids_from_another_vault_are_not_exposed_or_applied() {
    let temporary = tempfile::tempdir().unwrap();
    let context = context(temporary.path());
    let first = temporary.path().join("first");
    let second = temporary.path().join("second");
    initialize(&context, &first);
    initialize(&context, &second);
    let plan = kb_app::run(
        AppRequest::PlanCreate {
            vault: Some(second.display().to_string()),
            request: serde_json::from_value(knowledge_request()).unwrap(),
        },
        &context,
    )
    .unwrap();
    let operation_id = plan["operation_id"].as_str().unwrap();
    let server = start(context, &first, Some("secret"), true).await;

    for (method, path) in [
        ("GET", format!("/operations/{operation_id}")),
        ("GET", format!("/operations/{operation_id}/events")),
        ("POST", format!("/operations/{operation_id}/apply")),
    ] {
        let (status, response) = request(&server, method, &path, Some("secret"), "").await;
        assert_eq!(status, 403);
        assert_eq!(response["error"]["code"], "auth_denied");
    }
    assert!(!second.join("Wiki/articles/http.md").exists());
}

#[tokio::test]
async fn terminal_sse_stream_supports_reconnect_without_repeating_apply() {
    let temporary = tempfile::tempdir().unwrap();
    let context = context(temporary.path());
    let vault = temporary.path().join("vault");
    initialize(&context, &vault);
    let plan = kb_app::run(
        AppRequest::PlanCreate {
            vault: Some(vault.display().to_string()),
            request: serde_json::from_value(knowledge_request()).unwrap(),
        },
        &context,
    )
    .unwrap();
    let operation_id = plan["operation_id"].as_str().unwrap();
    let operation_id_value = operation_id.parse().unwrap();
    kb_app::run(
        AppRequest::Apply {
            operation_id: operation_id_value,
        },
        &context,
    )
    .unwrap();
    let report = kb_app::operation_events(
        &kb_app::UserPaths::new(
            temporary.path().join("config"),
            temporary.path().join("state"),
            temporary.path().join("cache"),
        ),
        operation_id_value,
    )
    .unwrap();
    let latest = report.events.last().unwrap().id;
    assert!(latest > 1);
    let server = start(context, &vault, Some("secret"), false).await;
    let path = format!("/operations/{operation_id}/events");

    let response = event_stream(&server, &path, Some("secret"), None).await;
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    assert!(response.contains("content-type: text/event-stream"));
    assert!(response.contains("event: operation"));
    assert!(response.contains("\"kind\":\"planned\""));
    assert!(response.contains("\"kind\":\"applied\""));

    let response = event_stream(&server, &path, Some("secret"), Some(latest - 1)).await;
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    assert!(response.contains(&format!("id: {latest}")));
    assert!(!response.contains("\"kind\":\"planned\""));

    let response = event_stream(&server, &path, Some("secret"), Some(latest + 1)).await;
    assert!(response.starts_with("HTTP/1.1 400"), "{response}");
    assert!(response.contains("invalid_config"));
}

#[tokio::test]
async fn active_sse_stream_observes_apply_and_closes_on_completion() {
    let temporary = tempfile::tempdir().unwrap();
    let context = context(temporary.path());
    let apply_context = context.clone();
    let vault = temporary.path().join("vault");
    initialize(&context, &vault);
    let plan = kb_app::run(
        AppRequest::PlanCreate {
            vault: Some(vault.display().to_string()),
            request: serde_json::from_value(knowledge_request()).unwrap(),
        },
        &context,
    )
    .unwrap();
    let operation_id = plan["operation_id"].as_str().unwrap().to_owned();
    let operation_id_value = operation_id.parse().unwrap();
    let server = start(context, &vault, None, false).await;
    let path = format!("/operations/{operation_id}/events");

    let stream = event_stream(&server, &path, None, None);
    let apply = async move {
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        tokio::task::spawn_blocking(move || {
            kb_app::run(
                AppRequest::Apply {
                    operation_id: operation_id_value,
                },
                &apply_context,
            )
        })
        .await
        .unwrap()
        .unwrap();
    };
    let (response, ()) = tokio::join!(stream, apply);

    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    assert!(response.contains("\"kind\":\"planned\""));
    assert!(response.contains("\"kind\":\"applying\""));
    assert!(response.contains("\"kind\":\"progress\""));
    assert!(response.contains("\"kind\":\"applied\""));
}

#[tokio::test]
async fn shutdown_signal_finishes_the_listener_cleanly() {
    let temporary = tempfile::tempdir().unwrap();
    let context = context(temporary.path());
    let vault = temporary.path().join("vault");
    initialize(&context, &vault);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let policy = ServerPolicy::new(address, None, false).unwrap();
    let state = ServerState::new(context, vault.display().to_string(), policy);

    serve(listener, state, async {}).await.unwrap();
}
