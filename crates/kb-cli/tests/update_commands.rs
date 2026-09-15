use std::path::Path;
use std::time::{Duration, Instant};

use assert_cmd::Command;
use sha2::{Digest, Sha256};

#[test]
fn development_binary_checks_managed_components_without_network_or_file_writes() {
    let temp = tempfile::tempdir().unwrap();
    let before = directory_entries(temp.path());

    let output = Command::cargo_bin("kb")
        .unwrap()
        .env("KB_CONFIG_DIR", temp.path().join("config"))
        .env("KB_STATE_DIR", temp.path().join("state"))
        .env("KB_CACHE_DIR", temp.path().join("cache"))
        .args(["update", "check", "--json"])
        .output()
        .unwrap();

    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let response: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(response["data"]["outcome"], "plan");
    assert_eq!(response["data"]["plan"]["state"], "no_changes");
    assert_eq!(
        response["data"]["plan"]["components"][0]["kind"],
        "executable"
    );
    assert_eq!(
        response["data"]["plan"]["components"][0]["state"],
        "skipped"
    );
    assert_eq!(directory_entries(temp.path()), before);
}

#[test]
fn development_binary_uses_bare_kb_update_as_the_prepare_command() {
    let temp = tempfile::tempdir().unwrap();
    let output = Command::cargo_bin("kb")
        .unwrap()
        .env("KB_CONFIG_DIR", temp.path().join("config"))
        .env("KB_STATE_DIR", temp.path().join("state"))
        .env("KB_CACHE_DIR", temp.path().join("cache"))
        .args(["update", "--json"])
        .output()
        .unwrap();

    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let response: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(response["data"]["outcome"], "plan");
    assert_eq!(
        response["data"]["plan"]["components"][0]["state"],
        "skipped"
    );
}

#[test]
fn hidden_helper_replaces_the_binary_and_cleans_its_private_stage() {
    let temp = tempfile::tempdir().unwrap();
    let executable = assert_cmd::cargo::cargo_bin!("kb");
    let executable_name = if cfg!(windows) { "kb.exe" } else { "kb" };
    let target = temp.path().join(executable_name);
    let backup = temp.path().join("kb.backup");
    let stage = temp.path().join(".kb-update-test");
    let verified = stage.join("verified");
    std::fs::create_dir_all(&verified).unwrap();
    std::fs::write(stage.join(".cleanup-token"), "test-token").unwrap();
    std::fs::copy(executable, &target).unwrap();
    let staged = verified.join(executable_name);
    std::fs::copy(executable, &staged).unwrap();
    let expected = hex::encode(Sha256::digest(std::fs::read(&staged).unwrap()));

    let output = std::process::Command::new(executable)
        .args([
            "__replace",
            "--parent-pid",
            &u32::MAX.to_string(),
            "--parent-start-time",
            "0",
            "--from",
        ])
        .arg(&staged)
        .arg("--to")
        .arg(&target)
        .arg("--backup")
        .arg(&backup)
        .arg("--expected-sha256")
        .arg(&expected)
        .arg("--cleanup-dir")
        .arg(&stage)
        .args(["--cleanup-token", "test-token", "--json"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    while stage.exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(25));
    }
    assert!(!stage.exists(), "cleanup helper left {}", stage.display());
    assert!(!backup.exists());
    assert_eq!(
        hex::encode(Sha256::digest(std::fs::read(target).unwrap())),
        expected
    );
}

#[test]
fn update_check_accepts_vault_scope_and_repeatable_exclusions() {
    let temp = tempfile::tempdir().unwrap();
    let vault = temp.path().join("vault");
    let report = kb_app::init_vault(&kb_app::InitRequest {
        target: vault.clone(),
    })
    .unwrap();

    let output = Command::cargo_bin("kb")
        .unwrap()
        .env("KB_CONFIG_DIR", temp.path().join("config"))
        .env("KB_STATE_DIR", temp.path().join("state"))
        .env("KB_CACHE_DIR", temp.path().join("cache"))
        .arg("update")
        .arg("check")
        .arg("--vault")
        .arg(&vault)
        .arg("--exclude-vault")
        .arg(report.vault_id.to_string())
        .arg("--json")
        .output()
        .unwrap();

    assert!(output.status.success());
    let response: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        response["data"]["plan"]["excluded_vaults"][0],
        report.vault_id.to_string()
    );
    assert_eq!(
        response["data"]["plan"]["scope"]["vaults"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
}

fn directory_entries(path: &Path) -> Vec<String> {
    std::fs::read_dir(path)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect()
}
