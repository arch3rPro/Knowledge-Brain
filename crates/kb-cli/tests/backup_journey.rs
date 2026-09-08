use assert_cmd::Command;
use serde_json::Value;
use std::{fs, path::Path};

#[test]
fn create_move_verify_restore_and_reopen_is_one_real_workflow() {
    let temporary = tempfile::tempdir().unwrap();
    let base = temporary.path();
    let vault = base.join("vault");
    let vault_text = vault.to_str().unwrap();
    initialize_captured_vault(base, &vault);

    fs::write(vault.join(".kb/runtime/source-pending.json"), "null").unwrap();
    assert_backup_blocked(base, vault_text);
    fs::remove_file(vault.join(".kb/runtime/source-pending.json")).unwrap();

    let created = run(
        base,
        base,
        &[
            "backup",
            "create",
            "--output",
            "vault.zip",
            "--vault",
            vault_text,
            "--json",
        ],
    );
    assert_eq!(created["data"]["complete_source_evidence"], true);
    assert!(base.join("vault.zip").is_file());

    let moved = base.join("moved");
    fs::create_dir(&moved).unwrap();
    fs::rename(base.join("vault.zip"), moved.join("portable.zip")).unwrap();
    let isolated_state = base.join("isolated-state");
    let verified = run(
        &moved,
        &isolated_state,
        &["backup", "verify", "portable.zip", "--json"],
    );
    assert_eq!(verified["data"]["vault_id"], created["data"]["vault_id"]);

    let restored = run(
        &moved,
        &isolated_state,
        &[
            "backup",
            "restore",
            "portable.zip",
            "--target",
            "restored",
            "--json",
        ],
    );
    assert_eq!(
        std::path::PathBuf::from(restored["data"]["target"].as_str().unwrap())
            .canonicalize()
            .unwrap(),
        moved.join("restored").canonicalize().unwrap()
    );
    assert_eq!(
        fs::read_to_string(moved.join("restored/Notes/portable.md")).unwrap(),
        "cross-platform-backup"
    );

    let query = run(
        &moved,
        &isolated_state,
        &[
            "query",
            "cross-platform-backup",
            "--scope",
            "all",
            "--vault",
            moved.join("restored").to_str().unwrap(),
            "--json",
        ],
    );
    assert!(
        query["data"]["groups"]
            .as_array()
            .unwrap()
            .iter()
            .any(|group| !group["results"].as_array().unwrap().is_empty())
    );
}

#[test]
fn default_backup_output_is_a_vault_sibling() {
    let temporary = tempfile::tempdir().unwrap();
    let vault = temporary.path().join("vault");
    run(
        temporary.path(),
        temporary.path(),
        &["init", vault.to_str().unwrap(), "--json"],
    );
    let created = run(
        temporary.path(),
        temporary.path(),
        &[
            "backup",
            "create",
            "--vault",
            vault.to_str().unwrap(),
            "--json",
        ],
    );
    let archive = std::path::PathBuf::from(created["data"]["archive"].as_str().unwrap());
    assert_eq!(archive.parent().unwrap(), vault.parent().unwrap());
    assert!(archive.is_file());
}

#[test]
fn backup_json_errors_distinguish_invalid_archives_from_restore_targets() {
    let temporary = tempfile::tempdir().unwrap();
    let base = temporary.path();
    let vault = base.join("vault");
    initialize_captured_vault(base, &vault);
    run(
        base,
        base,
        &[
            "backup",
            "create",
            "--output",
            "portable.zip",
            "--vault",
            vault.to_str().unwrap(),
            "--json",
        ],
    );

    let corrupt = base.join("corrupt.zip");
    fs::write(&corrupt, "not a ZIP archive").unwrap();
    let invalid = failure(
        base,
        base,
        &["backup", "verify", corrupt.to_str().unwrap(), "--json"],
    );
    assert_eq!(invalid["schema_version"], "v1.0");
    assert_eq!(invalid["error"]["code"], "backup_verification_failed");
    assert_eq!(invalid["error"]["details"]["legacy_code"], "restore_failed");

    let target = base.join("nonempty-target");
    fs::create_dir(&target).unwrap();
    fs::write(target.join("keep"), "keep").unwrap();
    let rejected = failure(
        base,
        base,
        &[
            "backup",
            "restore",
            "portable.zip",
            "--target",
            target.to_str().unwrap(),
            "--json",
        ],
    );
    assert_eq!(rejected["error"]["code"], "target_not_empty");
    assert_eq!(fs::read_to_string(target.join("keep")).unwrap(), "keep");
}

fn initialize_captured_vault(base: &Path, vault: &Path) {
    let vault_text = vault.to_str().unwrap();
    run(base, base, &["init", vault_text, "--json"]);
    fs::create_dir(vault.join("Notes")).unwrap();
    run(
        base,
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
    fs::write(vault.join("Notes/portable.md"), "cross-platform-backup").unwrap();
    let review = run(base, base, &["review", "--vault", vault_text, "--json"]);
    run(
        base,
        base,
        &[
            "apply",
            review["data"]["operation_id"].as_str().unwrap(),
            "--json",
        ],
    );
}

fn assert_backup_blocked(base: &Path, vault: &str) {
    let blocked = command(base, base)
        .args([
            "backup",
            "create",
            "--output",
            "blocked.zip",
            "--vault",
            vault,
            "--json",
        ])
        .output()
        .unwrap();
    assert!(!blocked.status.success());
    let blocked: Value = serde_json::from_slice(&blocked.stdout).unwrap();
    assert_eq!(blocked["error"]["code"], "vault_needs_recovery");
    assert!(!base.join("blocked.zip").exists());
}

fn run(current_dir: &Path, state_root: &Path, args: &[&str]) -> Value {
    let output = command(current_dir, state_root)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "command failed: {}\nstdout: {}\nstderr: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    serde_json::from_slice(&output.stdout).unwrap()
}

fn failure(current_dir: &Path, state_root: &Path, args: &[&str]) -> Value {
    let output = command(current_dir, state_root)
        .args(args)
        .output()
        .unwrap();
    assert!(
        !output.status.success(),
        "command unexpectedly succeeded: {}\nstdout: {}\nstderr: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    serde_json::from_slice(&output.stdout).unwrap()
}

fn command(current_dir: &Path, state_root: &Path) -> Command {
    let mut command = Command::cargo_bin("kb").unwrap();
    command
        .current_dir(current_dir)
        .env("KB_CONFIG_DIR", state_root.join("config"))
        .env("KB_STATE_DIR", state_root.join("state"))
        .env("KB_CACHE_DIR", state_root.join("cache"));
    command
}
