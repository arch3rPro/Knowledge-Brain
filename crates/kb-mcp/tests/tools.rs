use kb_app::{AppContext, AppRequest, InitRequest};
use kb_mcp::McpServer;
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs, path::Path};

#[test]
fn operation_show_preserves_plan_and_result_roots_with_additive_summary() {
    let temp = tempfile::tempdir().unwrap();
    let context = context(temp.path());
    let vault = temp.path().join("vault");
    let initialized = kb_app::run(
        AppRequest::Init(InitRequest {
            target: vault.clone(),
        }),
        &context,
    )
    .unwrap();
    let mut server = McpServer::new(
        context,
        initialized["vault_id"].as_str().unwrap().to_owned(),
        true,
    );

    let planned = call(
        &mut server,
        1,
        "kb_plan_knowledge",
        &json!({"request": knowledge_request()}),
    );
    assert_eq!(planned["result"]["isError"], false);
    let operation_id = planned["result"]["structuredContent"]["data"]["operation_id"]
        .as_str()
        .unwrap()
        .to_owned();

    let shown = call(
        &mut server,
        2,
        "kb_operation_show",
        &json!({"operation_id": operation_id}),
    );
    let data = &shown["result"]["structuredContent"]["data"];
    assert_eq!(shown["result"]["isError"], false);
    assert_eq!(data["state"], "planned");
    assert!(data["plan"].is_object());
    assert_eq!(data["operation_summary"]["requires_confirmation"], true);
    assert_eq!(data["operation_summary"]["operation_id"], operation_id);

    let applied = call(
        &mut server,
        3,
        "kb_apply_operation",
        &json!({"operation_id": operation_id}),
    );
    assert_eq!(applied["result"]["isError"], false);

    let shown = call(
        &mut server,
        4,
        "kb_operation_show",
        &json!({"operation_id": operation_id}),
    );
    let data = &shown["result"]["structuredContent"]["data"];
    assert_eq!(shown["result"]["isError"], false);
    assert_eq!(data["state"], "applied");
    assert!(data["result"].is_object());
    assert_eq!(data["operation_summary"]["operation_id"], operation_id);
    assert_eq!(data["operation_summary"]["can_apply"], false);
}

#[test]
fn fixed_vault_server_exposes_read_and_planning_tools_without_apply_by_default() {
    let temp = tempfile::tempdir().unwrap();
    let context = context(temp.path());
    let vault = temp.path().join("vault");
    let initialized =
        kb_app::run(AppRequest::Init(InitRequest { target: vault }), &context).unwrap();
    let vault_id = initialized["vault_id"].as_str().unwrap().to_owned();
    let mut server = McpServer::new(context, vault_id.clone(), false);

    let initialized = server
        .handle(&json!({
            "jsonrpc":"2.0", "id":1, "method":"initialize",
            "params":{"protocolVersion":"2026-07-28","capabilities":{},"clientInfo":{"name":"test","version":"1"}}
        }))
        .unwrap();
    assert_eq!(initialized["id"], 1);
    assert_eq!(initialized["result"]["protocolVersion"], "2025-06-18");
    assert_eq!(initialized["result"]["capabilities"]["tools"], json!({}));

    let listed = server
        .handle(&json!({"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}))
        .unwrap();
    let names = listed["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        names,
        vec![
            "kb_capabilities",
            "kb_status",
            "kb_maintenance",
            "kb_query",
            "kb_lint",
            "kb_review_sources",
            "kb_source_save",
            "kb_plan_knowledge",
            "kb_knowledge_save",
            "kb_operation_show",
        ]
    );
    assert!(!names.contains(&"kb_apply_operation"));
    let query_tool = listed["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == "kb_query")
        .unwrap();
    assert_eq!(
        query_tool["inputSchema"]["properties"]["match_mode"],
        json!({"type":"string","enum":["relevant","exact"],"default":"relevant"})
    );

    let exact = call(
        &mut server,
        3,
        "kb_query",
        &json!({"query":"needle","match_mode":"exact"}),
    );
    assert_eq!(
        exact["result"]["structuredContent"]["data"]["match_mode"],
        "exact"
    );

    let status = call(&mut server, 4, "kb_status", &json!({}));
    assert_eq!(status["result"]["isError"], false);
    assert_eq!(
        status["result"]["structuredContent"]["data"]["vault_id"],
        vault_id
    );

    let maintenance = call(&mut server, 5, "kb_maintenance", &json!({}));
    assert_eq!(maintenance["result"]["isError"], false);
    assert_eq!(
        maintenance["result"]["structuredContent"]["data"]["kind"],
        "maintenance"
    );
    assert!(
        maintenance["result"]["structuredContent"]["data"]
            .get("operation_id")
            .is_none()
    );

    let denied = call(
        &mut server,
        6,
        "kb_apply_operation",
        &json!({"operation_id":"c9af2059-734c-4ce8-b76a-4b68f20584a1"}),
    );
    assert_eq!(denied["error"]["code"], -32602);
}

#[test]
fn write_enabled_server_rejects_an_operation_owned_by_another_vault() {
    let temp = tempfile::tempdir().unwrap();
    let context = context(temp.path());
    let vault = temp.path().join("vault");
    let initialized =
        kb_app::run(AppRequest::Init(InitRequest { target: vault }), &context).unwrap();
    let other = temp.path().join("other");
    fs::create_dir(&other).unwrap();
    let foreign = kb_app::run(
        AppRequest::Adopt {
            target: other.clone(),
        },
        &context,
    )
    .unwrap();
    let mut server = McpServer::new(
        context,
        initialized["vault_id"].as_str().unwrap().to_owned(),
        true,
    );

    let listed = server
        .handle(&json!({"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}))
        .unwrap();
    assert!(
        listed["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .any(|tool| tool["name"] == "kb_apply_operation")
    );
    let response = call(
        &mut server,
        2,
        "kb_apply_operation",
        &json!({"operation_id": foreign["operation_id"]}),
    );
    assert_eq!(response["result"]["isError"], true);
    assert_eq!(
        response["result"]["structuredContent"]["error"]["code"],
        "auth_denied"
    );
    assert!(!other.join("KB.md").exists());
}

#[test]
fn source_save_prepares_read_only_and_confirms_only_with_write_access() {
    let temp = tempfile::tempdir().unwrap();
    let context = context(temp.path());
    let vault = temp.path().join("vault");
    let initialized = kb_app::run(
        AppRequest::Init(InitRequest {
            target: vault.clone(),
        }),
        &context,
    )
    .unwrap();
    fs::create_dir(vault.join("Notes")).unwrap();
    fs::write(vault.join("Notes/one.md"), "# One\n\nSource evidence.\n").unwrap();
    fs::write(
        vault.join("admission.yml"),
        "schema_version: v1.0\ndirectories:\n  - id: notes\n    path: Notes\n    enabled: true\n",
    )
    .unwrap();
    let vault_id = initialized["vault_id"].as_str().unwrap().to_owned();
    let mut read_only = McpServer::new(context.clone(), vault_id.clone(), false);

    let prepared = call(&mut read_only, 1, "kb_source_save", &json!({}));
    assert_eq!(prepared["result"]["isError"], false);
    assert_eq!(
        prepared["result"]["structuredContent"]["data"]["phase"],
        "awaiting_confirmation"
    );
    let token = prepared["result"]["structuredContent"]["data"]["confirmation_token"]
        .as_str()
        .unwrap()
        .to_owned();

    let denied = call(
        &mut read_only,
        2,
        "kb_source_save",
        &json!({"confirmation_token": token}),
    );
    assert_eq!(denied["result"]["isError"], true);
    assert_eq!(
        denied["result"]["structuredContent"]["error"]["code"],
        "auth_denied"
    );

    let mut writable = McpServer::new(context.clone(), vault_id, true);
    let applied = call(
        &mut writable,
        3,
        "kb_source_save",
        &json!({"confirmation_token": token}),
    );
    assert_eq!(applied["result"]["isError"], false);
    assert_eq!(
        applied["result"]["structuredContent"]["data"]["phase"],
        "applied"
    );
    let verified = kb_app::run(
        AppRequest::SourceVerify {
            vault: Some(vault.display().to_string()),
        },
        &context,
    )
    .unwrap();
    assert_eq!(verified["checks"][0]["status"], "pass");
}

fn call(server: &mut McpServer, id: u64, name: &str, arguments: &Value) -> Value {
    server
        .handle(&json!({
            "jsonrpc":"2.0", "id":id, "method":"tools/call",
            "params":{"name":name,"arguments":arguments}
        }))
        .unwrap()
}

fn knowledge_request() -> Value {
    json!({
        "schema_version": "v1.0",
        "changes": [{
            "path": "articles/mcp-contract.md",
            "before_sha256": null,
            "summary": "Add the MCP operation contract fixture.",
            "content": "---\ntype: Article\ntitle: MCP contract\nstatus: stable\ngenerated:\n  by: process:mcp-contract-test\n  at: 2026-09-08T03:00:00Z\nsources:\n  - id: source\n    resource: https://example.com/mcp-contract\nkb:\n  managed: true\n---\n\n# MCP contract\n"
        }]
    })
}

fn context(base: &Path) -> AppContext {
    AppContext::new(
        BTreeMap::from([
            (
                "KB_CONFIG_DIR".to_owned(),
                base.join("config").to_string_lossy().into_owned(),
            ),
            (
                "KB_STATE_DIR".to_owned(),
                base.join("state").to_string_lossy().into_owned(),
            ),
            (
                "KB_CACHE_DIR".to_owned(),
                base.join("cache").to_string_lossy().into_owned(),
            ),
        ]),
        base.to_path_buf(),
    )
}
