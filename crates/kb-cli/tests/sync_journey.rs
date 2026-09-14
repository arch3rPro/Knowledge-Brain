use std::path::Path;

use assert_cmd::Command;

#[test]
fn sync_check_uses_the_real_cli_and_shared_vault_identity() {
    let temporary = tempfile::tempdir().unwrap();
    let vault = temporary.path().join("Knowledge Vault");
    let initialized = run(
        temporary.path(),
        &["init", vault.to_str().unwrap(), "--json"],
    );
    let vault_id = initialized["data"]["vault_id"].as_str().unwrap();
    std::fs::remove_dir(vault.join(".kb/runtime")).unwrap();

    let by_path = run(
        temporary.path(),
        &[
            "sync",
            "check",
            "--vault",
            vault.to_str().unwrap(),
            "--json",
        ],
    );
    let by_id = run(
        temporary.path(),
        &["sync", "check", "--vault", vault_id, "--json"],
    );

    assert_eq!(by_path["data"]["vault_id"], vault_id);
    assert_eq!(by_id["data"]["vault_id"], vault_id);
    assert_eq!(by_path["data"]["root"], vault.to_str().unwrap());
    assert!(by_path["data"]["summary"]["writes_blocked"].is_boolean());
    assert!(by_path["data"]["findings"].is_array());

    let human = command(temporary.path())
        .args(["sync", "check", "--vault", vault.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(human.status.success());
    let text = String::from_utf8(human.stdout).unwrap();
    assert!(text.starts_with("同步检查\n"));
    assert!(text.contains("错误"));
    assert!(!text.contains("score"));
    assert!(!vault.join(".kb/runtime").exists());
}

#[test]
fn unresolved_managed_conflict_blocks_confirmed_config_write_but_not_preview() {
    let temporary = tempfile::tempdir().unwrap();
    let vault = temporary.path().join("vault");
    run(
        temporary.path(),
        &["init", vault.to_str().unwrap(), "--json"],
    );
    std::fs::write(
        vault.join("Wiki/index.md"),
        "# Index\n\n<<<<<<< ours\n=======\n>>>>>>> theirs\n",
    )
    .unwrap();
    let config = vault.join(".kb/config.yml");
    let before = std::fs::read(&config).unwrap();

    let preview = run(
        temporary.path(),
        &[
            "config",
            "set",
            "search.mode",
            "bm25",
            "--vault",
            vault.to_str().unwrap(),
            "--json",
        ],
    );
    assert_eq!(preview["data"]["written"], false);

    let output = command(temporary.path())
        .args([
            "config",
            "set",
            "search.mode",
            "bm25",
            "--vault",
            vault.to_str().unwrap(),
            "--yes",
            "--json",
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let response: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(response["error"]["code"], "sync_conflict");
    assert_eq!(std::fs::read(config).unwrap(), before);

    let local = run(
        temporary.path(),
        &[
            "config",
            "set",
            "search.mode",
            "bm25",
            "--local",
            "--vault",
            vault.to_str().unwrap(),
            "--yes",
            "--json",
        ],
    );
    assert_eq!(local["data"]["written"], true);
    assert!(vault.join(".kb/config.local.yml").is_file());
}

fn run(root: &Path, arguments: &[&str]) -> serde_json::Value {
    let output = command(root).args(arguments).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn command(root: &Path) -> Command {
    let mut command = Command::new(assert_cmd::cargo::cargo_bin!("kb"));
    command.current_dir(root).envs([
        ("KB_CONFIG_DIR", root.join("user-config")),
        ("KB_STATE_DIR", root.join("user-state")),
        ("KB_CACHE_DIR", root.join("user-cache")),
    ]);
    command
}
