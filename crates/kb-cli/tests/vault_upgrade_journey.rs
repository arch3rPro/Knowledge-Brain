use assert_cmd::Command;

const LEGACY_KB: &str =
    include_str!("../../../assets/vault-template-history/released-v0.1.x/KB.md");

#[test]
fn user_previews_and_confirms_a_legacy_vault_upgrade() {
    let temporary = tempfile::tempdir().unwrap();
    let vault = temporary.path().join("vault");
    run(
        temporary.path(),
        &["init", vault.to_str().unwrap(), "--json"],
    );
    std::fs::remove_file(vault.join(".kb/template.yml")).unwrap();
    std::fs::write(vault.join("KB.md"), LEGACY_KB).unwrap();

    let preview = run(
        temporary.path(),
        &[
            "vault",
            "upgrade",
            "--vault",
            vault.to_str().unwrap(),
            "--json",
        ],
    );

    assert_eq!(preview["data"]["kind"], "upgrade_vault");
    assert_eq!(preview["data"]["to_template_version"], "v1.3");
    assert_eq!(preview["data"]["conflicts"], serde_json::json!([]));
    assert!(
        preview["data"]["diff"]
            .as_str()
            .unwrap()
            .contains("+++ KB.md")
    );
    assert!(
        preview["data"]["writes"]
            .as_array()
            .unwrap()
            .iter()
            .all(|write| write.get("content").is_none())
    );
    assert_eq!(
        std::fs::read_to_string(vault.join("KB.md")).unwrap(),
        LEGACY_KB
    );
    let operation_id = preview["data"]["confirmation_token"].as_str().unwrap();

    let human_preview = command(temporary.path())
        .args(["vault", "upgrade", "--vault", vault.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(human_preview.status.success());
    assert!(
        String::from_utf8(human_preview.stdout)
            .unwrap()
            .contains("差异：")
    );

    let result = run(
        temporary.path(),
        &[
            "vault",
            "upgrade",
            "--vault",
            vault.to_str().unwrap(),
            "--confirm",
            operation_id,
            "--json",
        ],
    );

    assert_eq!(result["data"]["template_version"], "v1.3");
    assert!(vault.join(".kb/template.yml").is_file());
    assert!(
        std::fs::read_to_string(vault.join("KB.md"))
            .unwrap()
            .contains("<!-- kb:rules:start -->")
    );

    let output = command(temporary.path())
        .args(["vault", "upgrade", "--vault", vault.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(output.status.success());
    let human = String::from_utf8(output.stdout).unwrap();
    assert!(human.contains("Vault 模板升级预览"));
    assert!(human.contains("无需写入"));
}

#[test]
fn user_modified_legacy_rules_are_reported_and_cannot_be_confirmed() {
    let temporary = tempfile::tempdir().unwrap();
    let vault = temporary.path().join("vault");
    run(
        temporary.path(),
        &["init", vault.to_str().unwrap(), "--json"],
    );
    std::fs::remove_file(vault.join(".kb/template.yml")).unwrap();
    std::fs::write(vault.join("KB.md"), "# My rules\n").unwrap();

    let preview = run(
        temporary.path(),
        &[
            "vault",
            "upgrade",
            "--vault",
            vault.to_str().unwrap(),
            "--json",
        ],
    );

    assert!(preview["data"]["confirmation_token"].is_null());
    assert_eq!(preview["data"]["writes"], serde_json::json!([]));
    assert_eq!(preview["data"]["conflicts"][0]["path"], "KB.md");
    let operation_id = preview["data"]["operation_id"].as_str().unwrap();
    let output = command(temporary.path())
        .args([
            "vault",
            "upgrade",
            "--vault",
            vault.to_str().unwrap(),
            "--confirm",
            operation_id,
            "--json",
        ])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let error: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(error["error"]["code"], "invalid_config");
    assert_eq!(
        std::fs::read_to_string(vault.join("KB.md")).unwrap(),
        "# My rules\n"
    );
    assert!(!vault.join(".kb/template.yml").exists());
}

fn run(base: &std::path::Path, arguments: &[&str]) -> serde_json::Value {
    let output = command(base).args(arguments).output().unwrap();
    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn command(base: &std::path::Path) -> Command {
    let mut command = Command::cargo_bin("kb").unwrap();
    command
        .env("KB_CONFIG_DIR", base.join("config"))
        .env("KB_STATE_DIR", base.join("state"))
        .env("KB_CACHE_DIR", base.join("cache"));
    command
}
