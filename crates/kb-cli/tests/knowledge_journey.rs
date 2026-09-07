use assert_cmd::Command;
use serde_json::{Value, json};
use std::{fs, path::Path};

#[test]
// This journey keeps cache bytes and the replay that must preserve them in one flow.
#[allow(clippy::too_many_lines)]
fn plan_review_apply_query_and_reopen_are_one_real_workflow() {
    let temporary = tempfile::tempdir().unwrap();
    let base = temporary.path();
    let vault = base.join("vault");
    let vault_text = vault.to_str().unwrap();
    run(base, &["init", vault_text, "--json"]);
    fs::write(
        vault.join("Wiki/index.md"),
        "# Human index\n\nKeep me.\n\n<!-- kb:managed:start -->\n<!-- kb:managed:end -->\n",
    )
    .unwrap();
    fs::write(
        vault.join("Wiki/log.md"),
        "# Human log\n\nKeep me too.\n\n<!-- kb:managed:start -->\n<!-- kb:managed:end -->\n",
    )
    .unwrap();
    let request_path = base.join("request.json");
    fs::write(
        &request_path,
        serde_json::to_vec_pretty(&json!({
            "schema_version": "v1.0",
            "changes": [{
                "path": "articles/portable.md",
                "before_sha256": null,
                "summary": "Add the portable knowledge article.",
                "content": "---\ntype: Article\ntitle: Portable knowledge\nstatus: stable\ngenerated:\n  by: process:test\n  at: 2026-09-07T03:00:00Z\nsources:\n  - id: source\n    resource: https://example.com/source\nkb:\n  managed: true\n---\n\n# Portable knowledge\nreopenable-needle\n"
            }]
        }))
        .unwrap(),
    )
    .unwrap();
    let wiki_before = snapshot(&vault.join("Wiki"));

    let plan = run(
        base,
        &[
            "plan",
            "create",
            request_path.to_str().unwrap(),
            "--vault",
            vault_text,
            "--json",
        ],
    );
    assert_eq!(snapshot(&vault.join("Wiki")), wiki_before);
    let operation_id = plan["data"]["operation_id"].as_str().unwrap();
    let shown = run(base, &["operation", "show", operation_id, "--json"]);
    assert_eq!(shown["data"]["state"], "planned");
    assert_eq!(shown["data"]["plan"]["kind"], "save_knowledge");

    let first = run(base, &["apply", operation_id, "--json"]);
    run(
        base,
        &[
            "config",
            "set",
            "search.mode",
            "bm25",
            "--vault",
            vault_text,
            "--yes",
            "--json",
        ],
    );
    run(base, &["cache", "rebuild", "--vault", vault_text, "--json"]);
    let catalog_before_replay = fs::read(vault.join(".kb/cache/catalog.json")).unwrap();
    let bm25_before_replay = fs::read(vault.join(".kb/cache/bm25.json")).unwrap();
    let second = run(base, &["apply", operation_id, "--json"]);
    assert_eq!(second["data"], first["data"]);
    assert_eq!(
        fs::read(vault.join(".kb/cache/catalog.json")).unwrap(),
        catalog_before_replay
    );
    assert_eq!(
        fs::read(vault.join(".kb/cache/bm25.json")).unwrap(),
        bm25_before_replay
    );
    assert!(
        fs::read_to_string(vault.join("Wiki/index.md"))
            .unwrap()
            .contains("# Human index\n\nKeep me.")
    );
    let log = fs::read_to_string(vault.join("Wiki/log.md")).unwrap();
    assert!(log.contains("# Human log\n\nKeep me too."));
    assert_eq!(
        log.matches("Add the portable knowledge article.").count(),
        1
    );
    let query = run_from(
        &base.join("another-directory"),
        base,
        &[
            "query",
            "reopenable-needle",
            "--vault",
            vault_text,
            "--strict-backend",
            "--json",
        ],
    );
    assert_eq!(
        query["data"]["groups"][0]["results"][0]["path"],
        "Wiki/articles/portable.md"
    );
    assert_eq!(query["data"]["groups"][0]["results"][0]["backend"], "bm25f");
    let lint = run(base, &["lint", "--strict", "--vault", vault_text, "--json"]);
    assert_eq!(lint["data"]["findings"], json!([]));
}

#[test]
fn unsupported_schema_knowledge_apply_retry_preserves_vault_bytes() {
    let temporary = tempfile::tempdir().unwrap();
    let base = temporary.path();
    let vault = base.join("vault");
    let vault_text = vault.to_str().unwrap();
    run(base, &["init", vault_text, "--json"]);
    let request_path = base.join("request.json");
    fs::write(
        &request_path,
        serde_json::to_vec_pretty(&json!({
            "schema_version": "v1.0",
            "changes": [{
                "path": "articles/retry.md",
                "before_sha256": null,
                "summary": "Exercise an unsupported apply retry.",
                "content": "---\ntype: Article\ntitle: Retry\nstatus: stable\ngenerated:\n  by: process:test\n  at: 2026-09-07T03:00:00Z\nsources:\n  - id: source\n    resource: https://example.com/source\nkb:\n  managed: true\n---\n\n# Retry\n"
            }]
        }))
        .unwrap(),
    )
    .unwrap();
    let plan = run(
        base,
        &[
            "plan",
            "create",
            request_path.to_str().unwrap(),
            "--vault",
            vault_text,
            "--json",
        ],
    );
    let operation_id = plan["data"]["operation_id"].as_str().unwrap();
    run(base, &["apply", operation_id, "--json"]);

    fs::write(vault.join(".kb/cache/catalog.json"), b"catalog sentinel").unwrap();
    fs::write(vault.join(".kb/cache/bm25.json"), b"bm25 sentinel").unwrap();
    fs::write(
        vault.join(".kb/runtime/knowledge-pending.json"),
        serde_json::to_vec(operation_id).unwrap(),
    )
    .unwrap();
    let config_path = vault.join(".kb/config.yml");
    let unsupported_config = fs::read_to_string(&config_path)
        .unwrap()
        .replacen("v1.0", "v0.9", 1);
    fs::write(&config_path, unsupported_config).unwrap();
    let before = snapshot(&vault);

    let output = command(base)
        .args(["apply", operation_id, "--json"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(output.stderr.is_empty());
    let response: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(response["error"]["code"], "migration_unavailable");
    assert_eq!(snapshot(&vault), before);
}

#[test]
fn malformed_request_uses_the_json_error_contract() {
    let temporary = tempfile::tempdir().unwrap();
    let request = temporary.path().join("broken.json");
    fs::write(&request, "{broken").unwrap();
    let output = command(temporary.path())
        .args(["plan", "create", request.to_str().unwrap(), "--json"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(output.stderr.is_empty());
    let response: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(response["error"]["code"], "invalid_config");
}

fn run(base: &Path, arguments: &[&str]) -> Value {
    run_from(base, base, arguments)
}

fn run_from(current_dir: &Path, user_root: &Path, arguments: &[&str]) -> Value {
    fs::create_dir_all(current_dir).unwrap();
    let output = command(user_root)
        .current_dir(current_dir)
        .args(arguments)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "args={arguments:?}\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn command(base: &Path) -> Command {
    let mut command = Command::cargo_bin("kb").unwrap();
    command
        .env("KB_CONFIG_DIR", base.join("config"))
        .env("KB_STATE_DIR", base.join("state"))
        .env("KB_CACHE_DIR", base.join("cache"));
    command
}

fn snapshot(root: &Path) -> Vec<(String, Vec<u8>)> {
    let mut output = Vec::new();
    let mut entries = fs::read_dir(root)
        .unwrap()
        .map(Result::unwrap)
        .collect::<Vec<_>>();
    entries.sort_by_key(fs::DirEntry::file_name);
    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            for (nested, bytes) in snapshot(&path) {
                output.push((
                    format!("{}/{}", entry.file_name().to_string_lossy(), nested),
                    bytes,
                ));
            }
        } else {
            output.push((
                entry.file_name().to_string_lossy().into_owned(),
                fs::read(path).unwrap(),
            ));
        }
    }
    output
}
