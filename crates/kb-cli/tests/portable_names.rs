use std::{fs, path::Path};

use assert_cmd::Command;

#[test]
fn cli_rejects_nonportable_admission_names_and_collisions() {
    let temp = tempfile::tempdir().unwrap();
    let vault = temp.path().join("vault");
    run(temp.path(), &["init", vault.to_str().unwrap(), "--json"]);

    for path in ["nested/topic", "CON", "topic.", "topic "] {
        let response = fail(
            temp.path(),
            &[
                "config",
                "admission",
                "add",
                "bad",
                path,
                "--vault",
                vault.to_str().unwrap(),
                "--json",
            ],
        );
        assert_eq!(response["error"]["code"], "unsafe_path", "{path}");
    }

    for paths in [["Notes", "notes"], ["Café", "Cafe\u{301}"]] {
        fs::write(
            vault.join("admission.yml"),
            format!(
                "schema_version: \"v1.0\"\ndirectories:\n  - id: one\n    path: \"{}\"\n    enabled: false\n  - id: two\n    path: \"{}\"\n    enabled: false\n",
                paths[0], paths[1]
            ),
        )
        .unwrap();
        let response = fail(
            temp.path(),
            &[
                "config",
                "validate",
                "--vault",
                vault.to_str().unwrap(),
                "--json",
            ],
        );
        assert_eq!(response["error"]["code"], "invalid_config");
    }
}

#[test]
fn operation_paths_are_serialized_with_forward_slashes() {
    let temp = tempfile::tempdir().unwrap();
    let target = temp.path().join("existing");
    fs::create_dir(&target).unwrap();
    let plan = run(temp.path(), &["adopt", target.to_str().unwrap(), "--json"]);
    for file in plan["data"]["creates"].as_array().unwrap() {
        let path = file["relative_path"].as_str().unwrap();
        assert!(!path.contains('\\'));
    }
}

#[cfg(unix)]
#[test]
fn adoption_rejects_a_symbolic_link_fixture() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().unwrap();
    let target = temp.path().join("existing");
    let outside = temp.path().join("outside");
    fs::create_dir(&target).unwrap();
    fs::create_dir(&outside).unwrap();
    symlink(&outside, target.join("linked")).unwrap();
    let response = fail(temp.path(), &["adopt", target.to_str().unwrap(), "--json"]);
    assert_eq!(response["error"]["code"], "unsafe_path");
}

#[cfg(windows)]
#[test]
fn adoption_rejects_a_junction_fixture() {
    let temp = tempfile::tempdir().unwrap();
    let target = temp.path().join("existing");
    let outside = temp.path().join("outside");
    fs::create_dir(&target).unwrap();
    fs::create_dir(&outside).unwrap();
    let link = target.join("linked");
    let output = std::process::Command::new("cmd")
        .args([
            "/C",
            "mklink",
            "/J",
            link.to_str().unwrap(),
            outside.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    if !output.status.success() {
        eprintln!(
            "junction fixture skipped: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    let response = fail(temp.path(), &["adopt", target.to_str().unwrap(), "--json"]);
    assert_eq!(response["error"]["code"], "unsafe_path");
}

fn run(user_root: &Path, arguments: &[&str]) -> serde_json::Value {
    let output = command(user_root).args(arguments).output().unwrap();
    assert!(output.status.success());
    serde_json::from_slice(&output.stdout).unwrap()
}

fn fail(user_root: &Path, arguments: &[&str]) -> serde_json::Value {
    let output = command(user_root).args(arguments).output().unwrap();
    assert!(
        !output.status.success(),
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
