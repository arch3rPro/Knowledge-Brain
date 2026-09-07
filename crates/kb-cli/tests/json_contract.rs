use std::path::Path;

use assert_cmd::Command;

#[test]
fn invalid_config_is_stdout_clean_and_machine_readable() {
    let temp = tempfile::tempdir().unwrap();
    let vault = temp.path().join("vault");
    run(temp.path(), &["init", vault.to_str().unwrap(), "--json"]);
    std::fs::write(vault.join(".kb/config.yml"), b"schema_version: [broken\n").unwrap();

    let output = command(temp.path())
        .args(["status", "--vault", vault.to_str().unwrap(), "--json"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(output.stderr.is_empty());
    let body: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(body["schema_version"], "v1.0");
    assert_eq!(body["error"]["code"], "invalid_config");
}

#[test]
fn version_and_capabilities_are_explicit_contracts() {
    let temp = tempfile::tempdir().unwrap();
    let version = run(temp.path(), &["version", "--json"]);
    assert_eq!(version["data"]["schema_version"], "v1.0");
    assert!(version["data"]["app_version"].as_str().is_some());

    let capabilities = run(temp.path(), &["capabilities", "--json"]);
    assert_eq!(capabilities["data"]["direct_search"], true);
    assert_eq!(
        capabilities["data"]["extractors"],
        serde_json::json!([
            "builtin-text",
            "builtin-html",
            "builtin-epub",
            "builtin-docx"
        ])
    );
    assert_eq!(capabilities["data"]["bm25"], false);
    assert_eq!(capabilities["data"]["mcp"], false);
    assert_eq!(capabilities["data"]["http"], false);
    assert!(
        capabilities["data"]["commands"]
            .as_array()
            .unwrap()
            .contains(&serde_json::Value::String("status".to_owned()))
    );
    assert!(
        capabilities["data"]["commands"]
            .as_array()
            .unwrap()
            .contains(&serde_json::Value::String("lint".to_owned()))
    );
}

#[test]
fn status_classifies_schema_compatibility_without_guessing() {
    for (schema, expected) in [
        ("v0.9", "older_migratable"),
        ("v1.1", "newer_minor_read_only"),
        ("v2.0", "newer_major_diagnostic_only"),
    ] {
        let temp = tempfile::tempdir().unwrap();
        let vault = temp.path().join("vault");
        run(temp.path(), &["init", vault.to_str().unwrap(), "--json"]);
        let config_path = vault.join(".kb/config.yml");
        let config = std::fs::read_to_string(&config_path)
            .unwrap()
            .replacen("v1.0", schema, 1);
        std::fs::write(config_path, config).unwrap();

        let status = run(
            temp.path(),
            &["status", "--vault", vault.to_str().unwrap(), "--json"],
        );
        assert_eq!(status["data"]["schema"]["version"], schema);
        assert_eq!(status["data"]["schema"]["compatibility"], expected);
    }
}

#[test]
fn noncurrent_schemas_are_never_mutated() {
    for (schema, expected_error) in [
        ("v0.9", "migration_required"),
        ("v1.1", "schema_too_new"),
        ("v2.0", "schema_too_new"),
    ] {
        let temp = tempfile::tempdir().unwrap();
        let vault = temp.path().join("vault");
        run(temp.path(), &["init", vault.to_str().unwrap(), "--json"]);
        let config_path = vault.join(".kb/config.yml");
        let before = std::fs::read_to_string(&config_path)
            .unwrap()
            .replacen("v1.0", schema, 1);
        std::fs::write(&config_path, &before).unwrap();

        let output = command(temp.path())
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
        assert_eq!(response["error"]["code"], expected_error);
        assert_eq!(std::fs::read_to_string(&config_path).unwrap(), before);
    }
}

#[test]
fn older_schema_allows_reading_while_newer_schema_is_diagnostic_only() {
    for (schema, should_read) in [("v0.9", true), ("v1.1", false)] {
        let temp = tempfile::tempdir().unwrap();
        let vault = temp.path().join("vault");
        run(temp.path(), &["init", vault.to_str().unwrap(), "--json"]);
        let config_path = vault.join(".kb/config.yml");
        let config = std::fs::read_to_string(&config_path)
            .unwrap()
            .replacen("v1.0", schema, 1);
        std::fs::write(config_path, config).unwrap();
        let output = command(temp.path())
            .args([
                "config",
                "show",
                "--vault",
                vault.to_str().unwrap(),
                "--json",
            ])
            .output()
            .unwrap();
        assert_eq!(output.status.success(), should_read, "schema={schema}");
        if should_read {
            let response: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(response["data"]["schema_version"], schema);
        }
    }
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

fn command(user_root: &Path) -> Command {
    let mut command = Command::cargo_bin("kb").unwrap();
    command
        .env("KB_CONFIG_DIR", user_root.join("user-config"))
        .env("KB_STATE_DIR", user_root.join("user-state"))
        .env("KB_CACHE_DIR", user_root.join("user-cache"));
    command
}
