use std::fs;
use std::path::PathBuf;

use assert_cmd::Command;

const OLD_KB: &str = include_str!("../../../assets/vault-template-history/v1.1/KB.md");
const OLD_MANIFEST: &str = include_str!("../../../assets/vault-template-history/v1.1/template.yml");

struct Fixture {
    temp: tempfile::TempDir,
    vault: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let vault = temp.path().join("vault");
        let fixture = Self { temp, vault };
        let output = fixture
            .command()
            .arg("init")
            .arg(&fixture.vault)
            .output()
            .unwrap();
        assert!(output.status.success());
        fs::write(fixture.vault.join("KB.md"), OLD_KB).unwrap();
        fs::write(fixture.vault.join(".kb/template.yml"), OLD_MANIFEST).unwrap();
        fixture
    }

    fn command(&self) -> Command {
        let mut command = Command::cargo_bin("kb").unwrap();
        command
            .env("KB_CONFIG_DIR", self.temp.path().join("config"))
            .env("KB_STATE_DIR", self.temp.path().join("state"))
            .env("KB_CACHE_DIR", self.temp.path().join("cache"))
            .env("KB_AGENT_HOME", self.temp.path().join("home"))
            .env("KB_AGENT_CONFIG_DIR", self.temp.path().join("agent-config"));
        command
    }

    fn kb(&self) -> Vec<u8> {
        fs::read(self.vault.join("KB.md")).unwrap()
    }
}

#[test]
fn human_rejection_cancels_preview_without_writing_vault() {
    let fixture = Fixture::new();
    let before = fixture.kb();

    let output = fixture
        .command()
        .arg("update")
        .arg("--vault")
        .arg(&fixture.vault)
        .write_stdin("n\n")
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("更新计划"));
    assert!(stdout.contains("Apply this exact update plan? [y/N]"));
    assert!(stdout.contains("更新已取消，未执行任何变更。"));
    assert_eq!(fixture.kb(), before);

    let status = fixture
        .command()
        .args(["update", "status", "--json"])
        .output()
        .unwrap();
    assert!(status.status.success());
    let response: serde_json::Value = serde_json::from_slice(&status.stdout).unwrap();
    assert!(response["data"].is_null());
}

#[test]
fn end_of_input_is_cancellation_without_writing_vault() {
    let fixture = Fixture::new();
    let before = fixture.kb();

    let output = fixture
        .command()
        .arg("update")
        .arg("--vault")
        .arg(&fixture.vault)
        .write_stdin("")
        .output()
        .unwrap();

    assert!(output.status.success());
    assert_eq!(fixture.kb(), before);
    assert!(
        String::from_utf8(output.stdout)
            .unwrap()
            .contains("更新已取消，未执行任何变更。")
    );
}

#[test]
fn human_confirmation_applies_the_displayed_plan_once() {
    let fixture = Fixture::new();

    let output = fixture
        .command()
        .arg("update")
        .arg("--vault")
        .arg(&fixture.vault)
        .write_stdin("y\n")
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert_eq!(
        stdout
            .matches("Apply this exact update plan? [y/N]")
            .count(),
        1
    );
    assert!(stdout.contains("completed_with_skips"));
    assert!(
        fs::read_to_string(fixture.vault.join(".kb/template.yml"))
            .unwrap()
            .contains("template_version: v1.3")
    );
    assert!(fixture.vault.join(".kb/cache/catalog.json").is_file());
}

#[test]
fn json_prepare_requires_the_exact_explicit_confirmation_token() {
    let fixture = Fixture::new();
    let before = fixture.kb();
    let preview = fixture
        .command()
        .arg("update")
        .arg("--vault")
        .arg(&fixture.vault)
        .arg("--json")
        .output()
        .unwrap();
    assert!(preview.status.success());
    let preview: serde_json::Value = serde_json::from_slice(&preview.stdout).unwrap();
    let token = preview["data"]["confirmation_token"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(fixture.kb(), before);

    let wrong = fixture
        .command()
        .args(["update", "--confirm", &"0".repeat(64), "--json"])
        .output()
        .unwrap();
    assert!(!wrong.status.success());
    assert_eq!(fixture.kb(), before);

    let applied = fixture
        .command()
        .args(["update", "--confirm", &token, "--json"])
        .output()
        .unwrap();
    assert!(applied.status.success());
    let applied: serde_json::Value = serde_json::from_slice(&applied.stdout).unwrap();
    assert_eq!(applied["data"]["execution_state"], "completed_with_skips");
    let operation_id = applied["data"]["operation_id"].as_str().unwrap();

    let status = fixture
        .command()
        .args(["update", "status", operation_id, "--json"])
        .output()
        .unwrap();
    assert!(status.status.success());
    let status: serde_json::Value = serde_json::from_slice(&status.stdout).unwrap();
    assert_eq!(status["data"], applied["data"]);
}

#[test]
fn confirmation_rejects_scope_flags() {
    let fixture = Fixture::new();
    let output = fixture
        .command()
        .args(["update", "--confirm", &"0".repeat(64), "--vault"])
        .arg(&fixture.vault)
        .arg("--json")
        .output()
        .unwrap();
    assert!(!output.status.success());
    let response: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(response["error"]["code"], "invalid_config");
}

#[test]
fn multiple_vault_review_can_exclude_one_before_the_single_confirmation() {
    let fixture = Fixture::new();
    let second = fixture.temp.path().join("second-vault");
    let initialized = fixture.command().arg("init").arg(&second).output().unwrap();
    assert!(initialized.status.success());
    fs::write(second.join("KB.md"), OLD_KB).unwrap();
    fs::write(second.join(".kb/template.yml"), OLD_MANIFEST).unwrap();
    let config: serde_yaml_ng::Value =
        serde_yaml_ng::from_slice(&fs::read(second.join(".kb/config.yml")).unwrap()).unwrap();
    let excluded = config["vault_id"].as_str().unwrap();

    let output = fixture
        .command()
        .arg("update")
        .write_stdin(format!("{excluded}\ny\n"))
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert_eq!(
        stdout
            .matches("Exclude Vault IDs before confirmation")
            .count(),
        1
    );
    assert_eq!(
        stdout
            .matches("Apply this exact update plan? [y/N]")
            .count(),
        1
    );
    assert!(
        fs::read_to_string(fixture.vault.join(".kb/template.yml"))
            .unwrap()
            .contains("template_version: v1.3")
    );
    assert_eq!(fs::read_to_string(second.join("KB.md")).unwrap(), OLD_KB);
    assert_eq!(
        fs::read_to_string(second.join(".kb/template.yml")).unwrap(),
        OLD_MANIFEST
    );
}
