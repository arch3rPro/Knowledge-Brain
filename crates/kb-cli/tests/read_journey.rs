use assert_cmd::Command;

#[test]
fn query_result_can_be_read_to_completion_through_the_real_cli() {
    let temporary = tempfile::tempdir().unwrap();
    let vault = temporary.path().join("vault");
    run(
        temporary.path(),
        &["init", vault.to_str().unwrap(), "--json"],
    );
    let original = "# Long\n\n一二三四五六七八九十 remote-read-probe\n";
    std::fs::write(vault.join("Wiki/articles/long.md"), original).unwrap();

    let query = run(
        temporary.path(),
        &[
            "query",
            "remote-read-probe",
            "--exact",
            "--vault",
            vault.to_str().unwrap(),
            "--json",
        ],
    );
    let uri = query["data"]["groups"][0]["results"][0]["resource_uri"]
        .as_str()
        .unwrap();

    let first = run(
        temporary.path(),
        &[
            "read",
            uri,
            "--max-chars",
            "8",
            "--vault",
            vault.to_str().unwrap(),
            "--json",
        ],
    );
    assert_eq!(first["data"]["complete"], false);
    let cursor = first["data"]["next_cursor"].as_str().unwrap();
    let second = run(
        temporary.path(),
        &[
            "read",
            uri,
            "--cursor",
            cursor,
            "--max-chars",
            "100",
            "--vault",
            vault.to_str().unwrap(),
            "--json",
        ],
    );
    let reconstructed = format!(
        "{}{}",
        first["data"]["content"].as_str().unwrap(),
        second["data"]["content"].as_str().unwrap()
    );
    assert_eq!(reconstructed, original);
    assert_eq!(second["data"]["complete"], true);
}

fn run(base: &std::path::Path, args: &[&str]) -> serde_json::Value {
    let output = Command::cargo_bin("kb")
        .unwrap()
        .args(args)
        .env("KB_CONFIG_DIR", base.join("config"))
        .env("KB_STATE_DIR", base.join("state"))
        .env("KB_CACHE_DIR", base.join("cache"))
        .current_dir(base)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr={} stdout={}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
