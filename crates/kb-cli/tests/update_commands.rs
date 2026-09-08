use std::path::Path;
use std::time::{Duration, Instant};

use assert_cmd::Command;
use sha2::{Digest, Sha256};

#[test]
fn development_binary_refuses_update_without_network_or_file_writes() {
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

    assert!(!output.status.success());
    assert!(output.stderr.is_empty());
    let response: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(response["error"]["code"], "capability_unavailable");
    assert_eq!(directory_entries(temp.path()), before);
}

#[test]
fn development_binary_uses_kb_update_as_the_install_command() {
    let output = Command::cargo_bin("kb")
        .unwrap()
        .args(["update", "--json"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(output.stderr.is_empty());
    let response: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(response["error"]["code"], "capability_unavailable");
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

fn directory_entries(path: &Path) -> Vec<String> {
    std::fs::read_dir(path)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect()
}
