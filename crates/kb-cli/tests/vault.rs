use std::path::Path;

use assert_cmd::Command;

#[test]
fn initialized_vault_is_registered_and_can_be_rebound_after_a_move() {
    let temp = tempfile::tempdir().unwrap();
    let old = temp.path().join("old");
    let initialized = run(temp.path(), &["init", old.to_str().unwrap(), "--json"]);
    let vault_id = initialized["data"]["vault_id"].as_str().unwrap();

    let listed = run(temp.path(), &["vault", "list", "--json"]);
    assert_eq!(listed["data"]["vaults"][0]["vault_id"], vault_id);
    assert_eq!(listed["data"]["vaults"][0]["path"], old.to_str().unwrap());
    let implicit_paths = run(temp.path(), &["paths", "--json"]);
    assert_eq!(implicit_paths["data"]["root"], old.to_str().unwrap());

    let moved = temp.path().join("moved");
    std::fs::rename(&old, &moved).unwrap();
    let stale = run_failure(temp.path(), &["paths", "--vault", vault_id, "--json"]);
    assert_eq!(stale["error"]["code"], "vault_not_found");

    run(
        temp.path(),
        &[
            "vault",
            "rebind",
            vault_id,
            moved.to_str().unwrap(),
            "--json",
        ],
    );
    let paths = run(temp.path(), &["paths", "--vault", vault_id, "--json"]);
    assert_eq!(paths["data"]["root"], moved.to_str().unwrap());
    let expected_config = moved.join(".kb/config.yml");
    assert_eq!(paths["data"]["config"], expected_config.to_str().unwrap());

    run(temp.path(), &["vault", "unregister", vault_id, "--json"]);
    assert!(moved.join(".kb/config.yml").is_file());
    let listed = run(temp.path(), &["vault", "list", "--json"]);
    assert!(listed["data"]["vaults"].as_array().unwrap().is_empty());

    run(
        temp.path(),
        &["vault", "register", moved.to_str().unwrap(), "--json"],
    );
    let listed = run(temp.path(), &["vault", "list", "--json"]);
    assert_eq!(listed["data"]["vaults"][0]["vault_id"], vault_id);
}

#[test]
fn config_accepts_a_registered_vault_id() {
    let temp = tempfile::tempdir().unwrap();
    let vault = temp.path().join("vault");
    let initialized = run(temp.path(), &["init", vault.to_str().unwrap(), "--json"]);
    let vault_id = initialized["data"]["vault_id"].as_str().unwrap();

    let shown = run(
        temp.path(),
        &["config", "show", "--vault", vault_id, "--json"],
    );
    assert_eq!(shown["data"]["vault_id"], vault_id);
}

fn run(user_root: &Path, arguments: &[&str]) -> serde_json::Value {
    let output = command(user_root).args(arguments).output().unwrap();
    assert!(
        output.status.success(),
        "args={arguments:?}\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn run_failure(user_root: &Path, arguments: &[&str]) -> serde_json::Value {
    let output = command(user_root).args(arguments).output().unwrap();
    assert!(
        !output.status.success(),
        "args={arguments:?}\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn command(user_root: &Path) -> Command {
    let mut command = Command::cargo_bin("kb").unwrap();
    command
        .env("KB_CONFIG_DIR", user_root.join("user-config"))
        .env("KB_STATE_DIR", user_root.join("user-state"))
        .env("KB_CACHE_DIR", user_root.join("user-cache"));
    command
}
