use std::{fs, path::Path};

use assert_cmd::Command;
use serde_json::Value;

const ARTICLE: &str = "# Hash fixture\nknown-content\n";
const ARTICLE_SHA256: &str = "847b0a9fcd536043c9cf79fc50d96f4ec8ea0ba3d8ceff5598361b0c3bbcb954";

#[test]
fn standalone_hashes_are_short_by_default_and_full_only_when_requested() {
    let temporary = tempfile::tempdir().unwrap();
    let base = temporary.path();
    let vault = initialized_vault(base);
    fs::write(vault.join("Wiki/articles/hash.md"), ARTICLE).unwrap();

    let concise = run_text(
        base,
        &["cache", "rebuild", "--vault", vault.to_str().unwrap()],
    );
    assert!(concise.contains("sha256: 847b0a9fcd53"), "{concise}");
    assert!(!concise.contains(ARTICLE_SHA256), "{concise}");
    assert!(concise.contains("path: Wiki/articles/hash.md"), "{concise}");
    assert!(!concise.contains('{'), "{concise}");
    assert!(!concise.contains("\"sha256\""), "{concise}");

    let full = run_text(
        base,
        &[
            "cache",
            "rebuild",
            "--full-hashes",
            "--vault",
            vault.to_str().unwrap(),
        ],
    );
    assert!(
        full.contains(&format!("sha256: {ARTICLE_SHA256}")),
        "{full}"
    );

    let json = run_json(
        base,
        &[
            "cache",
            "rebuild",
            "--full-hashes",
            "--vault",
            vault.to_str().unwrap(),
            "--json",
        ],
    );
    let article = json["data"]["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["path"] == "Wiki/articles/hash.md")
        .unwrap();
    assert_eq!(article["sha256"], ARTICLE_SHA256);
}

#[test]
fn operation_plans_render_only_the_shared_labeled_summary_with_full_identities() {
    let temporary = tempfile::tempdir().unwrap();
    let base = temporary.path();
    let target = base.join("existing vault");
    fs::create_dir(&target).unwrap();
    fs::write(target.join("keep.md"), "human data\n").unwrap();

    let text = run_text(base, &["adopt", target.to_str().unwrap()]);

    assert!(text.contains("summary: Adopt Vault:"), "{text}");
    assert!(text.contains("operation_kind: adopt_vault"), "{text}");
    assert!(text.contains("operation_state: planned"), "{text}");
    assert!(
        text.contains(&format!("vault_root: {}", target.display())),
        "{text}"
    );
    assert!(text.contains("affected_paths:"), "{text}");
    assert!(text.contains("  - .kb/config.yml"), "{text}");
    assert!(text.contains("requires_confirmation: true"), "{text}");
    assert!(text.contains("can_apply: true"), "{text}");
    assert!(!text.contains("operation_summary:"), "{text}");
    assert!(!text.contains("creates:"), "{text}");
    assert!(!text.contains('{'), "{text}");

    let operation_id = labeled_value(&text, "operation_id");
    let vault_id = labeled_value(&text, "vault_id");
    assert_eq!(
        uuid::Uuid::parse_str(operation_id).unwrap().to_string(),
        operation_id
    );
    assert_eq!(
        uuid::Uuid::parse_str(vault_id).unwrap().to_string(),
        vault_id
    );
}

#[test]
fn query_groups_render_path_title_and_snippet_lines() {
    let temporary = tempfile::tempdir().unwrap();
    let base = temporary.path();
    let vault = initialized_vault(base);
    fs::write(
        vault.join("Wiki/articles/query.md"),
        "# Search title\n\nA concise-query-needle appears here.\n",
    )
    .unwrap();

    let text = run_text(
        base,
        &[
            "query",
            "concise-query-needle",
            "--vault",
            vault.to_str().unwrap(),
        ],
    );

    assert!(text.contains("path: Wiki/articles/query.md"), "{text}");
    assert!(text.contains("title: Search title"), "{text}");
    assert!(
        text.contains("snippet: A concise-query-needle appears here."),
        "{text}"
    );
    assert!(!text.contains("match_count"), "{text}");
    assert!(!text.contains('{'), "{text}");
}

#[test]
fn doctor_checks_render_as_status_id_and_message() {
    let temporary = tempfile::tempdir().unwrap();
    let base = temporary.path();
    let vault = initialized_vault(base);

    let text = run_text(base, &["doctor", "--vault", vault.to_str().unwrap()]);

    assert!(
        text.lines()
            .any(|line| line.starts_with("pass machine_runtime_directories: ")),
        "{text}"
    );
    assert!(
        text.lines()
            .any(|line| line.starts_with("pass vault_readability: ")),
        "{text}"
    );
    assert!(!text.contains("checks:"), "{text}");
    assert!(!text.contains('{'), "{text}");
}

#[test]
fn empty_source_verification_uses_the_generic_checks_renderer() {
    let temporary = tempfile::tempdir().unwrap();
    let base = temporary.path();
    let vault = initialized_vault(base);

    let text = run_text(
        base,
        &["source", "verify", "--vault", vault.to_str().unwrap()],
    );

    assert_eq!(text, "checks:\n  (none)\n");
}

#[test]
fn diff_output_remains_byte_for_byte_identical_to_the_response_diff() {
    let temporary = tempfile::tempdir().unwrap();
    let base = temporary.path();
    let vault = initialized_vault(base);
    let arguments = [
        "config",
        "set",
        "search.mode",
        "bm25",
        "--vault",
        vault.to_str().unwrap(),
    ];

    let json = run_json_with_suffix(base, &arguments, &["--json"]);
    let diff = json["data"]["diff"].as_str().unwrap();
    let output = command(base).args(arguments).output().unwrap();

    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    assert_eq!(output.stdout, format!("{diff}\n").as_bytes());
}

#[test]
fn maintenance_reports_real_checks_without_persisting_a_plan() {
    let temporary = tempfile::tempdir().unwrap();
    let base = temporary.path();
    let vault = initialized_vault(base);
    fs::create_dir(vault.join("Notes")).unwrap();
    run_json(
        base,
        &[
            "config",
            "admission",
            "add",
            "notes",
            "Notes",
            "--vault",
            vault.to_str().unwrap(),
            "--yes",
            "--json",
        ],
    );
    fs::write(vault.join("Notes/new.md"), "# New source\n").unwrap();
    let vault_before = file_snapshot(&vault);
    let state_before = file_snapshot(&base.join("state"));

    let report = run_json(
        base,
        &["maintain", "--vault", vault.to_str().unwrap(), "--json"],
    );

    assert_eq!(report["data"]["kind"], "maintenance");
    assert_eq!(report["data"]["sources"]["changes"][0]["kind"], "added");
    assert!(report["data"].get("operation_id").is_none());
    assert!(!report["data"].to_string().contains("operation_id"));
    assert_eq!(file_snapshot(&vault), vault_before);
    assert_eq!(file_snapshot(&base.join("state")), state_before);

    let text = run_text(base, &["maintain", "--vault", vault.to_str().unwrap()]);
    assert!(text.contains("sources: 1 change(s) (added 1)"), "{text}");
    assert!(text.contains("lint:"), "{text}");
    assert!(text.contains("doctor:"), "{text}");
    assert!(!text.contains("operation"), "{text}");
}

#[test]
fn maintenance_reports_recovery_without_claiming_skipped_checks_passed() {
    let temporary = tempfile::tempdir().unwrap();
    let base = temporary.path();
    let vault = initialized_vault(base);
    fs::write(vault.join(".kb/runtime/source-pending.json"), "{}\n").unwrap();
    fs::write(vault.join(".kb/runtime/vault.lock"), "").unwrap();
    let before = file_snapshot(&vault);

    let report = run_json(
        base,
        &["maintain", "--vault", vault.to_str().unwrap(), "--json"],
    );
    assert_eq!(
        report["data"]["status"]["recovery"]["pending_operations"],
        1
    );
    assert_eq!(report["data"]["sources"]["status"], "not_checked");
    assert_eq!(report["data"]["lint"]["status"], "not_checked");
    assert_eq!(file_snapshot(&vault), before);

    let text = run_text(base, &["maintain", "--vault", vault.to_str().unwrap()]);
    assert!(text.contains("recovery: 1 pending operation(s)"), "{text}");
    assert!(text.contains("sources: not checked"), "{text}");
    assert!(text.contains("lint: not checked"), "{text}");
    assert!(!text.contains("sources: no changes"), "{text}");
    assert!(!text.contains("lint: no findings"), "{text}");
}

fn file_snapshot(root: &Path) -> Vec<(std::path::PathBuf, Vec<u8>)> {
    fn collect(root: &Path, current: &Path, files: &mut Vec<(std::path::PathBuf, Vec<u8>)>) {
        let Ok(entries) = fs::read_dir(current) else {
            return;
        };
        for entry in entries {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.is_dir() {
                collect(root, &path, files);
            } else {
                files.push((
                    path.strip_prefix(root).unwrap().to_path_buf(),
                    fs::read(path).unwrap(),
                ));
            }
        }
    }
    let mut files = Vec::new();
    collect(root, root, &mut files);
    files.sort_by(|left, right| left.0.cmp(&right.0));
    files
}

fn initialized_vault(base: &Path) -> std::path::PathBuf {
    let vault = base.join("vault");
    run_json(base, &["init", vault.to_str().unwrap(), "--json"]);
    vault
}

fn labeled_value<'a>(text: &'a str, label: &str) -> &'a str {
    text.lines()
        .find_map(|line| line.strip_prefix(&format!("{label}: ")))
        .unwrap()
}

fn run_text(base: &Path, arguments: &[&str]) -> String {
    let output = command(base).args(arguments).output().unwrap();
    assert!(
        output.status.success(),
        "args={arguments:?}\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    String::from_utf8(output.stdout).unwrap()
}

fn run_json(base: &Path, arguments: &[&str]) -> Value {
    let output = command(base).args(arguments).output().unwrap();
    assert!(
        output.status.success(),
        "args={arguments:?}\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    serde_json::from_slice(&output.stdout).unwrap()
}

fn run_json_with_suffix(base: &Path, arguments: &[&str], suffix: &[&str]) -> Value {
    let mut command = command(base);
    command.args(arguments).args(suffix);
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
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
