use assert_cmd::Command;

#[test]
fn config_commands_preserve_text_and_report_effective_sources() {
    let temp = tempfile::tempdir().unwrap();
    let vault = temp.path().join("vault");
    run(&["init", vault.to_str().unwrap(), "--json"]);
    let config_path = vault.join(".kb/config.yml");
    let original = std::fs::read_to_string(&config_path).unwrap();
    std::fs::write(
        &config_path,
        format!("# owner note\ncustom: keep # inline\n{original}"),
    )
    .unwrap();

    let preview = run(&[
        "config",
        "set",
        "search.mode",
        "bm25",
        "--vault",
        vault.to_str().unwrap(),
        "--json",
    ]);
    assert_eq!(preview["data"]["written"], false);
    assert!(
        preview["data"]["diff"]
            .as_str()
            .unwrap()
            .contains("+  mode: bm25")
    );
    assert!(
        std::fs::read_to_string(&config_path)
            .unwrap()
            .contains("mode: direct")
    );

    let changed = run(&[
        "config",
        "set",
        "search.mode",
        "bm25",
        "--vault",
        vault.to_str().unwrap(),
        "--yes",
        "--json",
    ]);
    assert_eq!(changed["data"]["written"], true);
    let after = std::fs::read_to_string(&config_path).unwrap();
    assert!(after.starts_with("# owner note\n"));
    assert!(after.contains("custom: keep # inline\n"));
    assert!(after.contains("mode: bm25\n"));

    let shown = run(&[
        "config",
        "show",
        "--sources",
        "--vault",
        vault.to_str().unwrap(),
        "--json",
    ]);
    assert_eq!(shown["data"]["search"]["mode"]["value"], "bm25");
    assert_eq!(shown["data"]["search"]["mode"]["source"], "vault");

    let fetched = run(&[
        "config",
        "get",
        "search.mode",
        "--vault",
        vault.to_str().unwrap(),
        "--json",
    ]);
    assert_eq!(fetched["data"]["key"], "search.mode");
    assert_eq!(fetched["data"]["value"], "bm25");

    run(&[
        "config",
        "unset",
        "search.mode",
        "--vault",
        vault.to_str().unwrap(),
        "--yes",
        "--json",
    ]);
    let after_unset = run(&[
        "config",
        "show",
        "--sources",
        "--vault",
        vault.to_str().unwrap(),
        "--json",
    ]);
    assert_eq!(after_unset["data"]["search"]["mode"]["value"], "direct");
    assert_eq!(after_unset["data"]["search"]["mode"]["source"], "built_in");
}

#[test]
fn admission_commands_manage_only_existing_top_level_directories() {
    let temp = tempfile::tempdir().unwrap();
    let vault = temp.path().join("vault");
    run(&["init", vault.to_str().unwrap(), "--json"]);
    std::fs::create_dir(vault.join("Notes")).unwrap();

    let added = run(&[
        "config",
        "admission",
        "add",
        "notes",
        "Notes",
        "--vault",
        vault.to_str().unwrap(),
        "--yes",
        "--json",
    ]);
    assert_eq!(added["data"]["written"], true);
    let listed = run(&[
        "config",
        "admission",
        "list",
        "--vault",
        vault.to_str().unwrap(),
        "--json",
    ]);
    assert_eq!(listed["data"]["directories"][0]["id"], "notes");
    assert_eq!(listed["data"]["directories"][0]["path"], "Notes");
    assert_eq!(listed["data"]["directories"][0]["enabled"], true);

    run(&[
        "config",
        "admission",
        "disable",
        "notes",
        "--vault",
        vault.to_str().unwrap(),
        "--yes",
        "--json",
    ]);
    let disabled = run(&[
        "config",
        "admission",
        "list",
        "--vault",
        vault.to_str().unwrap(),
        "--json",
    ]);
    assert_eq!(disabled["data"]["directories"][0]["enabled"], false);
    let validated = run(&[
        "config",
        "validate",
        "--vault",
        vault.to_str().unwrap(),
        "--json",
    ]);
    assert_eq!(validated["data"]["valid"], true);
    assert_eq!(validated["data"]["admission_entries"], 1);

    run(&[
        "config",
        "admission",
        "remove",
        "notes",
        "--vault",
        vault.to_str().unwrap(),
        "--yes",
        "--json",
    ]);
    let admission = std::fs::read_to_string(vault.join("admission.yml")).unwrap();
    assert!(admission.contains("directories: []"));
    assert!(vault.join("Notes").is_dir());
}

#[test]
fn config_set_validates_the_complete_candidate_before_writing() {
    let temp = tempfile::tempdir().unwrap();
    let vault = temp.path().join("vault");
    run(&["init", vault.to_str().unwrap(), "--json"]);
    let config_path = vault.join(".kb/config.yml");
    let before = std::fs::read(&config_path).unwrap();

    let output = Command::cargo_bin("kb")
        .unwrap()
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
        .env("KB_LIMITS_MAX_FILE_BYTES", "0")
        .output()
        .unwrap();

    assert!(!output.status.success());
    let response: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(response["error"]["code"], "invalid_config");
    assert_eq!(std::fs::read(&config_path).unwrap(), before);
}

#[test]
fn config_set_rejects_multiple_target_layers() {
    let temp = tempfile::tempdir().unwrap();
    let vault = temp.path().join("vault");
    run(&["init", vault.to_str().unwrap(), "--json"]);

    let output = Command::cargo_bin("kb")
        .unwrap()
        .args([
            "config",
            "set",
            "search.mode",
            "bm25",
            "--vault",
            vault.to_str().unwrap(),
            "--local",
            "--user",
            "--yes",
        ])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("cannot be used with"));
}

fn run(arguments: &[&str]) -> serde_json::Value {
    let output = Command::cargo_bin("kb")
        .unwrap()
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
