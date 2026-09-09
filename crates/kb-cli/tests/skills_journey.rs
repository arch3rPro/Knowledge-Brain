use assert_cmd::Command;
use serde_json::Value;
use std::{fs, path::Path};

#[test]
fn real_cli_installs_reports_and_safely_uninstalls_the_portable_skill() {
    let temp = tempfile::tempdir().unwrap();
    let vault = temp.path().join("vault");
    run(temp.path(), &["init", vault.to_str().unwrap(), "--json"]);
    let original = "# Existing Agent rules\n";
    fs::write(vault.join("AGENTS.md"), original).unwrap();

    let detected = run(
        temp.path(),
        &[
            "skills",
            "detect",
            "--vault",
            vault.to_str().unwrap(),
            "--json",
        ],
    );
    assert_eq!(detected["data"]["detected"].as_array().unwrap().len(), 2);

    let planned = run(
        temp.path(),
        &[
            "skills",
            "install",
            "--host",
            "codex",
            "--scope",
            "vault",
            "--mode",
            "copy",
            "--vault",
            vault.to_str().unwrap(),
            "--json",
        ],
    );
    let id = planned["data"]["operation_id"].as_str().unwrap();
    assert!(!vault.join(".agents/skills/knowledge-brain").exists());
    let shown = run(temp.path(), &["operation", "show", id, "--json"]);
    assert_eq!(shown["data"]["state"], "planned");

    run(temp.path(), &["apply", id, "--json"]);
    let status = run(
        temp.path(),
        &[
            "skills",
            "status",
            "--host",
            "codex",
            "--scope",
            "vault",
            "--vault",
            vault.to_str().unwrap(),
            "--json",
        ],
    );
    assert_eq!(status["data"]["state"], "current");

    let uninstall = run(
        temp.path(),
        &[
            "skills",
            "uninstall",
            "--host",
            "codex",
            "--scope",
            "vault",
            "--vault",
            vault.to_str().unwrap(),
            "--json",
        ],
    );
    let uninstall_id = uninstall["data"]["operation_id"].as_str().unwrap();
    run(temp.path(), &["apply", uninstall_id, "--json"]);
    assert_eq!(
        fs::read_to_string(vault.join("AGENTS.md")).unwrap(),
        original
    );
    assert!(!vault.join(".agents/skills/knowledge-brain").exists());
}

#[test]
fn real_cli_installs_nested_assets_for_a_native_host_without_a_bridge() {
    let temp = tempfile::tempdir().unwrap();
    let vault = temp.path().join("vault");
    run(temp.path(), &["init", vault.to_str().unwrap(), "--json"]);

    let planned = run(
        temp.path(),
        &[
            "skills",
            "install",
            "--host",
            "pi-coding-agent",
            "--scope",
            "vault",
            "--vault",
            vault.to_str().unwrap(),
            "--json",
        ],
    );
    assert!(planned["data"]["bridge_file"].is_null());
    let id = planned["data"]["operation_id"].as_str().unwrap();
    run(temp.path(), &["apply", id, "--json"]);

    let request_format = vault.join(".pi/skills/kb-save/references/request-format.md");
    assert_eq!(
        fs::read_to_string(&request_format).unwrap(),
        include_str!("../../../skills/kb-save/references/request-format.md")
    );
    assert!(!vault.join("AGENTS.md").exists());

    let status = run(
        temp.path(),
        &[
            "skills",
            "status",
            "--host",
            "pi",
            "--scope",
            "vault",
            "--vault",
            vault.to_str().unwrap(),
            "--json",
        ],
    );
    assert_eq!(status["data"]["host"], "pi");
    assert_eq!(status["data"]["state"], "current");

    let uninstall = run(
        temp.path(),
        &[
            "skills",
            "uninstall",
            "--host",
            "pi",
            "--scope",
            "vault",
            "--vault",
            vault.to_str().unwrap(),
            "--json",
        ],
    );
    let uninstall_id = uninstall["data"]["operation_id"].as_str().unwrap();
    run(temp.path(), &["apply", uninstall_id, "--json"]);
    assert!(!request_format.exists());
}

fn run(user_root: &Path, arguments: &[&str]) -> Value {
    let output = Command::cargo_bin("kb")
        .unwrap()
        .env("KB_CONFIG_DIR", user_root.join("config"))
        .env("KB_STATE_DIR", user_root.join("state"))
        .env("KB_CACHE_DIR", user_root.join("cache"))
        .env("KB_AGENT_HOME", user_root.join("home"))
        .env("KB_AGENT_CONFIG_DIR", user_root.join("agent-config"))
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
