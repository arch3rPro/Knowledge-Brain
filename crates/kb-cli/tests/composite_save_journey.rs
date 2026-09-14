use assert_cmd::Command;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{fs, path::Path};

#[test]
fn source_save_prepares_then_confirms_the_same_capture() {
    let temporary = tempfile::tempdir().unwrap();
    let base = temporary.path();
    let vault = base.join("vault");
    let vault_text = vault.to_str().unwrap();
    run(base, &["init", vault_text, "--json"]);
    fs::create_dir(vault.join("Notes")).unwrap();
    fs::write(vault.join("Notes/one.md"), "# One\n\nSource evidence.\n").unwrap();
    run(
        base,
        &[
            "config",
            "admission",
            "add",
            "notes",
            "Notes",
            "--vault",
            vault_text,
            "--yes",
            "--json",
        ],
    );

    let prepared = run(base, &["source", "save", "--vault", vault_text, "--json"]);
    assert_eq!(prepared["data"]["phase"], "awaiting_confirmation");
    assert_eq!(
        prepared["data"]["change_summary"]["affected_paths"],
        json!(["Notes/one.md"])
    );
    let token = prepared["data"]["confirmation_token"].as_str().unwrap();

    let applied = run(
        base,
        &[
            "source",
            "save",
            "--confirm",
            token,
            "--vault",
            vault_text,
            "--json",
        ],
    );
    assert_eq!(applied["data"]["phase"], "applied");
    let verified = run(base, &["source", "verify", "--vault", vault_text, "--json"]);
    assert_eq!(verified["data"]["checks"][0]["status"], "pass");
}

#[test]
fn knowledge_save_previews_confirms_and_supports_direct_authorization() {
    let temporary = tempfile::tempdir().unwrap();
    let base = temporary.path();
    let vault = base.join("vault");
    let vault_text = vault.to_str().unwrap();
    run(base, &["init", vault_text, "--json"]);
    let request = base.join("request.json");
    fs::write(&request, serde_json::to_vec(&knowledge_request()).unwrap()).unwrap();

    let prepared = run(
        base,
        &[
            "knowledge",
            "save",
            request.to_str().unwrap(),
            "--vault",
            vault_text,
            "--json",
        ],
    );
    assert_eq!(prepared["data"]["phase"], "awaiting_confirmation");
    assert!(!vault.join("Wiki/articles/composite.md").exists());
    let token = prepared["data"]["confirmation_token"].as_str().unwrap();
    let applied = run(
        base,
        &[
            "knowledge",
            "save",
            "--confirm",
            token,
            "--vault",
            vault_text,
            "--json",
        ],
    );
    assert_eq!(applied["data"]["phase"], "applied");
    assert!(vault.join("Wiki/articles/composite.md").is_file());

    let direct_vault = base.join("direct");
    let direct_text = direct_vault.to_str().unwrap();
    run(base, &["init", direct_text, "--json"]);
    let direct = run(
        base,
        &[
            "knowledge",
            "save",
            request.to_str().unwrap(),
            "--yes",
            "--vault",
            direct_text,
            "--json",
        ],
    );
    assert_eq!(direct["data"]["phase"], "applied");
    assert!(direct_vault.join("Wiki/articles/composite.md").is_file());
}

#[test]
fn human_save_preview_shows_only_the_change_summary() {
    let temporary = tempfile::tempdir().unwrap();
    let base = temporary.path();
    let vault = base.join("vault");
    let vault_text = vault.to_str().unwrap();
    run(base, &["init", vault_text, "--json"]);
    fs::create_dir(vault.join("Notes")).unwrap();
    fs::write(vault.join("Notes/one.md"), "# One\n\nSource evidence.\n").unwrap();
    run(
        base,
        &[
            "config",
            "admission",
            "add",
            "notes",
            "Notes",
            "--vault",
            vault_text,
            "--yes",
            "--json",
        ],
    );

    let output = command(base)
        .args(["source", "save", "--vault", vault_text])
        .output()
        .unwrap();

    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("summary:"), "{text}");
    assert!(text.contains("Notes/one.md"), "{text}");
    assert!(!text.contains("confirmation_token"), "{text}");
    assert!(!text.contains("operation_id"), "{text}");
    assert!(!text.contains("preview:"), "{text}");
}

#[test]
fn knowledge_save_moves_and_deletes_managed_pages_through_the_normal_cli_flow() {
    let temporary = tempfile::tempdir().unwrap();
    let base = temporary.path();
    let vault = base.join("vault");
    let vault_text = vault.to_str().unwrap();
    run(base, &["init", vault_text, "--json"]);
    let old = "---\ntype: Research\ntitle: Old title\nstatus: stable\ngenerated:\n  by: process:test\n  at: 2026-09-08T00:00:00Z\nsources:\n  - id: source\n    resource: https://example.com\nkb:\n  managed: true\n---\n\n# Old title\n";
    let obsolete = old.replace("Old title", "Obsolete");
    fs::write(vault.join("Wiki/research/old.md"), old).unwrap();
    fs::write(vault.join("Wiki/articles/obsolete.md"), &obsolete).unwrap();
    let request = base.join("move-delete.json");
    fs::write(
        &request,
        serde_json::to_vec(&json!({
            "schema_version": "v1.0",
            "changes": [
                {
                    "kind": "move",
                    "from_path": "research/old.md",
                    "path": "research/new.md",
                    "before_sha256": hex::encode(Sha256::digest(old.as_bytes())),
                    "summary": "Rename the research page.",
                    "content": old.replace("Old title", "New title")
                },
                {
                    "kind": "delete",
                    "path": "articles/obsolete.md",
                    "before_sha256": hex::encode(Sha256::digest(obsolete.as_bytes())),
                    "summary": "Remove obsolete knowledge."
                }
            ]
        }))
        .unwrap(),
    )
    .unwrap();

    let applied = run(
        base,
        &[
            "knowledge",
            "save",
            request.to_str().unwrap(),
            "--yes",
            "--vault",
            vault_text,
            "--json",
        ],
    );

    assert_eq!(applied["data"]["phase"], "applied");
    assert!(!vault.join("Wiki/research/old.md").exists());
    assert!(vault.join("Wiki/research/new.md").is_file());
    assert!(!vault.join("Wiki/articles/obsolete.md").exists());
    let log = fs::read_to_string(vault.join("Wiki/log.md")).unwrap();
    assert!(log.contains("**Move**"));
    assert!(log.contains("**Deletion**"));
}

fn knowledge_request() -> Value {
    json!({
        "schema_version": "v1.0",
        "changes": [{
            "path": "articles/composite.md",
            "before_sha256": null,
            "summary": "Save composite knowledge.",
            "content": "---\ntype: Article\ntitle: Composite save\nstatus: stable\ngenerated:\n  by: process:composite-save-cli-test\n  at: 2026-09-08T00:00:00Z\nsources:\n  - id: source\n    resource: https://example.com/composite\nkb:\n  managed: true\n---\n\n# Composite save\n"
        }]
    })
}

fn run(base: &Path, arguments: &[&str]) -> Value {
    let output = command(base).args(arguments).output().unwrap();
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
        .current_dir(base)
        .env("KB_CONFIG_DIR", base.join("config"))
        .env("KB_STATE_DIR", base.join("state"))
        .env("KB_CACHE_DIR", base.join("cache"));
    command
}
