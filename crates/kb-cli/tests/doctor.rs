use std::{collections::BTreeMap, path::Path};

use assert_cmd::Command;

#[test]
fn doctor_reports_independent_checks_without_editing_vault_data() {
    let temp = tempfile::tempdir().unwrap();
    let vault = temp.path().join("vault");
    run(temp.path(), &["init", vault.to_str().unwrap(), "--json"]);
    let before = snapshot_data(&vault);

    let report = run(
        temp.path(),
        &["doctor", "--vault", vault.to_str().unwrap(), "--json"],
    );

    assert!(report["data"].get("score").is_none());
    let checks = report["data"]["checks"].as_array().unwrap();
    for id in [
        "machine_runtime_directories",
        "vault_readability",
        "vault_structure",
        "vault_writability",
        "path_portability",
        "configuration",
        "lock_acquisition",
        "recovery_records",
        "optional_integrations",
    ] {
        assert!(checks.iter().any(|check| check["id"] == id), "{id}");
    }
    assert!(checks.iter().all(|check| matches!(
        check["status"].as_str(),
        Some("pass" | "warn" | "fail" | "not_checked")
    )));
    let runtime = checks
        .iter()
        .find(|check| check["id"] == "machine_runtime_directories")
        .unwrap();
    assert_eq!(runtime["status"], "pass");
    assert!(
        runtime["message"]
            .as_str()
            .unwrap()
            .contains("created on demand")
    );
    assert_eq!(snapshot_data(&vault), before);
    assert!(!vault.join(".kb/runtime/vault.lock").exists());
}

#[test]
fn doctor_human_output_names_vault_and_machine_runtime_checks() {
    let temp = tempfile::tempdir().unwrap();
    let vault = temp.path().join("vault");
    run(temp.path(), &["init", vault.to_str().unwrap(), "--json"]);

    let output = Command::cargo_bin("kb")
        .unwrap()
        .env("KB_CONFIG_DIR", temp.path().join("user-config"))
        .env("KB_STATE_DIR", temp.path().join("user-state"))
        .env("KB_CACHE_DIR", temp.path().join("user-cache"))
        .args(["doctor", "--vault", vault.to_str().unwrap()])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(
        text.lines()
            .any(|line| line.starts_with("pass machine_runtime_directories: ")),
        "{text}"
    );
    assert!(
        text.lines()
            .any(|line| line.starts_with("pass vault_structure: ")),
        "{text}"
    );
    assert!(!text.contains("standard_directories"), "{text}");
}

#[test]
fn doctor_contains_a_configuration_failure_instead_of_collapsing_the_report() {
    let temp = tempfile::tempdir().unwrap();
    let vault = temp.path().join("vault");
    run(temp.path(), &["init", vault.to_str().unwrap(), "--json"]);
    std::fs::write(vault.join(".kb/config.yml"), b"schema_version: [broken\n").unwrap();

    let report = run(
        temp.path(),
        &["doctor", "--vault", vault.to_str().unwrap(), "--json"],
    );

    let configuration = report["data"]["checks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|check| check["id"] == "configuration")
        .unwrap();
    assert_eq!(configuration["status"], "fail");
}

#[test]
fn doctor_warns_when_schema_has_no_migration_path() {
    let temp = tempfile::tempdir().unwrap();
    let vault = temp.path().join("vault");
    run(temp.path(), &["init", vault.to_str().unwrap(), "--json"]);
    let config_path = vault.join(".kb/config.yml");
    let config = std::fs::read_to_string(&config_path)
        .unwrap()
        .replacen("v1.0", "v0.9", 1);
    std::fs::write(config_path, config).unwrap();

    let report = run(
        temp.path(),
        &["doctor", "--vault", vault.to_str().unwrap(), "--json"],
    );

    let configuration = report["data"]["checks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|check| check["id"] == "configuration")
        .unwrap();
    assert_eq!(configuration["status"], "warn");
    assert!(
        configuration["message"]
            .as_str()
            .unwrap()
            .contains("no migration path")
    );
}

fn run(user_root: &Path, arguments: &[&str]) -> serde_json::Value {
    let output = Command::cargo_bin("kb")
        .unwrap()
        .env("KB_CONFIG_DIR", user_root.join("user-config"))
        .env("KB_STATE_DIR", user_root.join("user-state"))
        .env("KB_CACHE_DIR", user_root.join("user-cache"))
        .args(arguments)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn snapshot_data(root: &Path) -> BTreeMap<String, Vec<u8>> {
    [
        "admission.yml",
        "KB.md",
        "Wiki/index.md",
        "Wiki/log.md",
        ".kb/config.yml",
    ]
    .into_iter()
    .map(|relative| {
        (
            relative.to_owned(),
            std::fs::read(root.join(relative)).unwrap(),
        )
    })
    .collect()
}
