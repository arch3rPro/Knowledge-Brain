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
        "standard_directories",
        "vault_readability",
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
    assert_eq!(snapshot_data(&vault), before);
    assert!(!vault.join(".kb/runtime/vault.lock").exists());
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
