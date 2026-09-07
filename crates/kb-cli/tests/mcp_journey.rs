use assert_cmd::Command;
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Read, Write},
    path::Path,
    process::{Command as ProcessCommand, Stdio},
};

#[test]
fn write_enabled_mcp_applies_an_approved_plan_and_the_result_survives_restart() {
    let temp = tempfile::tempdir().unwrap();
    let vault = temp.path().join("vault");
    run_cli(temp.path(), &["init", vault.to_str().unwrap(), "--json"]);

    let mut child = mcp_process(temp.path(), &vault, true);
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    send(
        &mut stdin,
        json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"kb_plan_knowledge","arguments":{"request":{"schema_version":"v1.0","changes":[{"path":"articles/mcp-write.md","before_sha256":null,"summary":"Exercise MCP write path","content":"---\ntype: Article\ntitle: MCP write journey\nstatus: stable\ngenerated:\n  by: process:mcp-test\n  at: 2026-09-07T03:00:00Z\nsources:\n  - id: mcp-spec\n    resource: https://modelcontextprotocol.io/\nkb:\n  managed: true\n---\n\n# MCP write journey\n\npersisted-mcp-needle\n"}]}}}}),
    );
    let planned = receive(&mut stdout);
    let operation_id = planned["result"]["structuredContent"]["data"]["operation_id"]
        .as_str()
        .unwrap();
    send(
        &mut stdin,
        json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"kb_apply_operation","arguments":{"operation_id":operation_id}}}),
    );
    let applied = receive(&mut stdout);
    assert_eq!(applied["result"]["isError"], false, "{applied}");
    drop(stdin);
    assert!(child.wait().unwrap().success());
    let mut stderr = String::new();
    child
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut stderr)
        .unwrap();
    assert_eq!(stderr, "");
    assert!(vault.join("Wiki/articles/mcp-write.md").is_file());

    let mut restarted = mcp_process(temp.path(), &vault, false);
    let mut restarted_stdin = restarted.stdin.take().unwrap();
    let mut restarted_stdout = BufReader::new(restarted.stdout.take().unwrap());
    send(
        &mut restarted_stdin,
        json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"kb_query","arguments":{"query":"persisted-mcp-needle","scope":"wiki"}}}),
    );
    let queried = receive(&mut restarted_stdout);
    assert_eq!(queried["result"]["isError"], false, "{queried}");
    assert!(
        queried["result"]["structuredContent"]["data"]["groups"][0]["results"]
            .as_array()
            .unwrap()
            .iter()
            .any(|result| result["path"] == "Wiki/articles/mcp-write.md")
    );
    drop(restarted_stdin);
    assert!(restarted.wait().unwrap().success());
}

#[test]
fn real_mcp_process_negotiates_queries_plans_and_denies_apply_by_default() {
    let temp = tempfile::tempdir().unwrap();
    let vault = temp.path().join("vault");
    run_cli(temp.path(), &["init", vault.to_str().unwrap(), "--json"]);

    let mut child = ProcessCommand::new(assert_cmd::cargo::cargo_bin!("kb"))
        .env("KB_CONFIG_DIR", temp.path().join("config"))
        .env("KB_STATE_DIR", temp.path().join("state"))
        .env("KB_CACHE_DIR", temp.path().join("cache"))
        .args(["mcp", "--vault", vault.to_str().unwrap()])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    let requests = [
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"1"}}}),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}),
        json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"kb_status","arguments":{}}}),
        json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"kb_plan_knowledge","arguments":{"request":{"schema_version":"v1.0","changes":[{"path":"articles/mcp.md","before_sha256":null,"summary":"Add MCP note","content":"---\ntype: Article\ntitle: MCP\nstatus: draft\ngenerated:\n  by: process:mcp-test\n  at: 2026-09-07T03:00:00Z\nsources:\n  - id: mcp-spec\n    resource: https://modelcontextprotocol.io/\nkb:\n  managed: true\n---\n\n# MCP\n"}]}}}}),
        json!({"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"kb_apply_operation","arguments":{"operation_id":"00000000-0000-0000-0000-000000000000"}}}),
    ];
    {
        let stdin = child.stdin.as_mut().unwrap();
        for request in requests {
            writeln!(stdin, "{}", serde_json::to_string(&request).unwrap()).unwrap();
        }
    }
    drop(child.stdin.take());

    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stderr), "");
    let responses = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(responses.len(), 5);
    assert_eq!(responses[0]["result"]["protocolVersion"], "2025-06-18");
    assert!(
        responses[1]["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .all(|tool| tool["name"] != "kb_apply_operation")
    );
    assert_eq!(responses[2]["result"]["isError"], false);
    assert_eq!(
        responses[2]["result"]["structuredContent"]["schema_version"],
        "v1.0"
    );
    assert_eq!(responses[3]["result"]["isError"], false, "{}", responses[3]);
    assert!(responses[3]["result"]["structuredContent"]["data"]["operation_id"].is_string());
    assert_eq!(responses[4]["error"]["code"], -32602);
    assert!(!vault.join("Wiki/articles/mcp.md").exists());
}

fn run_cli(user_root: &Path, arguments: &[&str]) -> Value {
    let output = Command::cargo_bin("kb")
        .unwrap()
        .env("KB_CONFIG_DIR", user_root.join("config"))
        .env("KB_STATE_DIR", user_root.join("state"))
        .env("KB_CACHE_DIR", user_root.join("cache"))
        .args(arguments)
        .output()
        .unwrap();
    assert!(output.status.success());
    serde_json::from_slice(&output.stdout).unwrap()
}

fn mcp_process(user_root: &Path, vault: &Path, allow_write: bool) -> std::process::Child {
    let mut command = ProcessCommand::new(assert_cmd::cargo::cargo_bin!("kb"));
    command
        .env("KB_CONFIG_DIR", user_root.join("config"))
        .env("KB_STATE_DIR", user_root.join("state"))
        .env("KB_CACHE_DIR", user_root.join("cache"))
        .args(["mcp", "--vault", vault.to_str().unwrap()]);
    if allow_write {
        command.arg("--allow-write");
    }
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap()
}

fn send(writer: &mut impl Write, request: Value) {
    writeln!(writer, "{}", serde_json::to_string(&request).unwrap()).unwrap();
    writer.flush().unwrap();
}

fn receive(reader: &mut impl BufRead) -> Value {
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    serde_json::from_str(&line).unwrap()
}
