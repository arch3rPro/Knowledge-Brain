use assert_cmd::Command;

#[test]
fn status_discovers_vault_from_a_descendant() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("vault");
    let nested = root.join("Notes/meetings");
    Command::cargo_bin("kb")
        .unwrap()
        .env("KB_CONFIG_DIR", temp.path().join("user-config"))
        .env("KB_STATE_DIR", temp.path().join("user-state"))
        .env("KB_CACHE_DIR", temp.path().join("user-cache"))
        .args(["init", root.to_str().unwrap(), "--json"])
        .assert()
        .success();
    std::fs::create_dir_all(&nested).unwrap();

    Command::cargo_bin("kb")
        .unwrap()
        .env("KB_CONFIG_DIR", temp.path().join("user-config"))
        .env("KB_STATE_DIR", temp.path().join("user-state"))
        .env("KB_CACHE_DIR", temp.path().join("user-cache"))
        .current_dir(nested)
        .args(["status", "--json"])
        .assert()
        .success();
}
