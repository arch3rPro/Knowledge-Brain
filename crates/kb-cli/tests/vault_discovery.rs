use assert_cmd::Command;

#[test]
#[ignore = "status command is implemented in Task 10"]
fn status_discovers_vault_from_a_descendant() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("vault");
    let nested = root.join("Notes/meetings");
    std::fs::create_dir_all(root.join(".kb")).unwrap();
    std::fs::create_dir_all(&nested).unwrap();
    std::fs::write(
        root.join(".kb/config.yml"),
        "schema_version: \"v1.0\"\nvault_id: \"20e4d3b4-5f9a-4d61-8ed0-c8f86bbda352\"\n",
    )
    .unwrap();

    Command::cargo_bin("kb")
        .unwrap()
        .current_dir(nested)
        .args(["status", "--json"])
        .assert()
        .success();
}
