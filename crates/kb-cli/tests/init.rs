use assert_cmd::Command;

#[test]
fn init_creates_only_the_minimum_vault() {
    let temp = tempfile::tempdir().unwrap();
    let vault = temp.path().join("PortableVault");

    let output = command(temp.path())
        .args(["init", vault.to_str().unwrap(), "--json"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let response: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(response["schema_version"], "v1.0");
    assert_eq!(response["data"]["root"], vault.to_str().unwrap());

    for relative in [
        "admission.yml",
        "KB.md",
        "Wiki/index.md",
        "Wiki/log.md",
        ".kb/config.yml",
        ".kb/schemas/admission.schema.json",
        ".kb/schemas/config.schema.json",
    ] {
        assert!(vault.join(relative).is_file(), "{relative}");
    }
    for relative in [
        "Wiki/external-sources/.objects/sha256",
        "Wiki/research",
        "Wiki/articles",
        ".kb/cache",
        ".kb/runtime",
    ] {
        assert!(vault.join(relative).is_dir(), "{relative}");
    }

    let config: serde_yaml_ng::Value =
        serde_yaml_ng::from_slice(&std::fs::read(vault.join(".kb/config.yml")).unwrap()).unwrap();
    assert_eq!(config["schema_version"].as_str(), Some("v1.0"));
    assert!(uuid::Uuid::parse_str(config["vault_id"].as_str().unwrap()).is_ok());
    assert_eq!(config["search"]["mode"].as_str(), Some("direct"));

    assert!(!vault.join("AI-Toolkit").exists());
    assert!(!vault.join(".git").exists());
    assert!(!vault.join(".kb/config.local.yml").exists());
}

#[test]
fn init_rejects_a_nonempty_target_without_changing_it() {
    let temp = tempfile::tempdir().unwrap();
    let vault = temp.path().join("existing");
    std::fs::create_dir(&vault).unwrap();
    std::fs::write(vault.join("keep.md"), b"human text\n").unwrap();

    let output = command(temp.path())
        .args(["init", vault.to_str().unwrap(), "--json"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let response: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(response["error"]["code"], "target_not_empty");

    assert_eq!(
        std::fs::read(vault.join("keep.md")).unwrap(),
        b"human text\n"
    );
    assert!(!vault.join(".kb").exists());
}

#[test]
fn init_treats_a_hidden_file_as_existing_content() {
    let temp = tempfile::tempdir().unwrap();
    let vault = temp.path().join("existing");
    std::fs::create_dir(&vault).unwrap();
    std::fs::write(vault.join(".keep"), b"preserve\n").unwrap();

    let output = command(temp.path())
        .args(["init", vault.to_str().unwrap(), "--json"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let response: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(response["error"]["code"], "target_not_empty");
    assert_eq!(std::fs::read(vault.join(".keep")).unwrap(), b"preserve\n");
}

#[test]
fn registration_failure_does_not_remove_an_initialized_vault() {
    let temp = tempfile::tempdir().unwrap();
    let vault = temp.path().join("vault");
    let output = Command::cargo_bin("kb")
        .unwrap()
        .args(["init", vault.to_str().unwrap(), "--json"])
        .env("KB_CONFIG_DIR", "relative-config-is-invalid")
        .env("KB_STATE_DIR", temp.path().join("user-state"))
        .env("KB_CACHE_DIR", temp.path().join("user-cache"))
        .output()
        .unwrap();

    assert!(output.status.success());
    let response: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(vault.join(".kb/config.yml").is_file());
    assert!(
        response["data"]["warnings"][0]
            .as_str()
            .unwrap()
            .contains("kb vault register")
    );
}

#[cfg(unix)]
#[test]
fn init_rejects_a_symbolic_link_target() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().unwrap();
    let real = temp.path().join("real");
    let link = temp.path().join("link");
    std::fs::create_dir(&real).unwrap();
    symlink(&real, &link).unwrap();

    let output = command(temp.path())
        .args(["init", link.to_str().unwrap(), "--json"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let response: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(response["error"]["code"], "unsafe_path");
    assert!(std::fs::read_dir(&real).unwrap().next().is_none());
}

fn command(user_root: &std::path::Path) -> Command {
    let mut command = Command::cargo_bin("kb").unwrap();
    command
        .env("KB_CONFIG_DIR", user_root.join("user-config"))
        .env("KB_STATE_DIR", user_root.join("user-state"))
        .env("KB_CACHE_DIR", user_root.join("user-cache"));
    command
}
