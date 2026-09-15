use std::{collections::BTreeMap, fs, path::Path, process::Command};

use serde_json::Value;

const OLD_KB: &str = include_str!("../../../assets/vault-template-history/v1.1/KB.md");
const OLD_MANIFEST: &str = include_str!("../../../assets/vault-template-history/v1.1/template.yml");

#[test]
fn packaged_candidate_updates_managed_state_and_reopens_with_durable_status() {
    let Some(binary) = std::env::var_os("KB_TEST_BINARY") else {
        return;
    };
    let temp = tempfile::tempdir().unwrap();
    let vault = temp.path().join("vault");
    let environment = environment(temp.path());
    run(
        &binary,
        &environment,
        &["init", vault.to_str().unwrap(), "--json"],
    );

    let skill = run(
        &binary,
        &environment,
        &[
            "skills",
            "install",
            "--host",
            "codex",
            "--scope",
            "vault",
            "--vault",
            vault.to_str().unwrap(),
            "--json",
        ],
    );
    run(
        &binary,
        &environment,
        &[
            "apply",
            skill["data"]["operation_id"].as_str().unwrap(),
            "--json",
        ],
    );

    fs::write(
        vault.join("KB.md"),
        format!("{OLD_KB}\nUser rules outside the managed block.\n"),
    )
    .unwrap();
    fs::write(vault.join(".kb/template.yml"), OLD_MANIFEST).unwrap();
    fs::create_dir(vault.join("Notes")).unwrap();
    fs::write(vault.join("Notes/user.md"), "user-owned-note").unwrap();
    fs::create_dir_all(vault.join(".agents/skills/external")).unwrap();
    fs::write(
        vault.join(".agents/skills/external/SKILL.md"),
        "external-skill",
    )
    .unwrap();

    let preview = run(
        &binary,
        &environment,
        &["update", "--vault", vault.to_str().unwrap(), "--json"],
    );
    assert_eq!(preview["data"]["outcome"], "plan");
    let token = preview["data"]["confirmation_token"].as_str().unwrap();
    assert!(
        preview["data"]["plan"]["components"]
            .as_array()
            .unwrap()
            .iter()
            .any(|component| component["kind"] == "vault_template"
                && component["state"] == "pending")
    );

    let applied = run(
        &binary,
        &environment,
        &["update", "--confirm", token, "--json"],
    );
    let state = applied["data"]["execution_state"].as_str().unwrap();
    assert!(matches!(state, "completed" | "completed_with_skips"));
    let operation_id = applied["data"]["operation_id"].as_str().unwrap();

    let reopened = run(
        &binary,
        &environment,
        &["update", "status", operation_id, "--json"],
    );
    assert_eq!(reopened["data"], applied["data"]);
    let version = run(&binary, &environment, &["version", "--json"]);
    assert_eq!(version["data"]["app_version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(version["data"]["distribution"]["official_release"], true);

    let kb = fs::read_to_string(vault.join("KB.md")).unwrap();
    assert!(kb.contains("User rules outside the managed block."));
    assert_eq!(
        fs::read_to_string(vault.join("Notes/user.md")).unwrap(),
        "user-owned-note"
    );
    assert_eq!(
        fs::read_to_string(vault.join(".agents/skills/external/SKILL.md")).unwrap(),
        "external-skill"
    );
    assert!(vault.join(".kb/cache/catalog.json").is_file());
}

fn environment(base: &Path) -> BTreeMap<&'static str, std::path::PathBuf> {
    BTreeMap::from([
        ("KB_CONFIG_DIR", base.join("config")),
        ("KB_STATE_DIR", base.join("state")),
        ("KB_CACHE_DIR", base.join("cache")),
        ("KB_AGENT_HOME", base.join("home")),
        ("KB_AGENT_CONFIG_DIR", base.join("agent-config")),
    ])
}

fn run(
    binary: &std::ffi::OsStr,
    environment: &BTreeMap<&str, std::path::PathBuf>,
    arguments: &[&str],
) -> Value {
    let mut command = Command::new(binary);
    command.env_clear();
    for name in [
        "PATH",
        "SystemRoot",
        "WINDIR",
        "TEMP",
        "TMP",
        "HOME",
        "USERPROFILE",
    ] {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    for (name, value) in environment {
        command.env(name, value);
    }
    let output = command.args(arguments).output().unwrap();
    assert!(
        output.status.success(),
        "args={arguments:?}\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
