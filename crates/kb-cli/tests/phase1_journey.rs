use std::{collections::BTreeMap, fs, path::Path};

use assert_cmd::Command;

#[test]
fn initialized_vault_survives_configuration_move_rebind_and_diagnostics() {
    let temp = tempfile::tempdir().unwrap();
    let original = temp.path().join("new-vault");
    let initialized = run(temp.path(), &["init", original.to_str().unwrap(), "--json"]);
    let vault_id = initialized["data"]["vault_id"].as_str().unwrap();
    fs::create_dir(original.join("Notes")).unwrap();

    run(
        temp.path(),
        &[
            "config",
            "admission",
            "add",
            "notes",
            "Notes",
            "--vault",
            original.to_str().unwrap(),
            "--yes",
            "--json",
        ],
    );
    let admission: serde_yaml_ng::Value =
        serde_yaml_ng::from_slice(&fs::read(original.join("admission.yml")).unwrap()).unwrap();
    assert_eq!(admission["directories"][0]["id"], "notes");

    let config = run(
        temp.path(),
        &[
            "config",
            "show",
            "--sources",
            "--vault",
            original.to_str().unwrap(),
            "--json",
        ],
    );
    assert_eq!(config["data"]["search"]["mode"]["source"], "vault");
    let status = run(
        temp.path(),
        &["status", "--vault", original.to_str().unwrap(), "--json"],
    );
    assert_eq!(status["data"]["admission"]["enabled"], 1);

    let moved = temp.path().join("moved-vault");
    fs::rename(&original, &moved).unwrap();
    run(
        temp.path(),
        &[
            "vault",
            "rebind",
            vault_id,
            moved.to_str().unwrap(),
            "--json",
        ],
    );
    let reopened = run(temp.path(), &["status", "--vault", vault_id, "--json"]);
    assert_eq!(reopened["data"]["root"], moved.to_str().unwrap());
    let doctor = run(temp.path(), &["doctor", "--vault", vault_id, "--json"]);
    assert!(doctor["data"]["checks"].as_array().unwrap().len() >= 8);
}

#[test]
fn adopted_directory_preserves_every_existing_byte_and_reopens() {
    let temp = tempfile::tempdir().unwrap();
    let target = temp.path().join("existing");
    fs::create_dir_all(target.join("Notes")).unwrap();
    fs::create_dir(target.join("Personal-disabled")).unwrap();
    fs::write(target.join("Notes/keep.md"), b"# Existing\n\nHuman text.\n").unwrap();
    fs::write(
        target.join("Personal-disabled/private.txt"),
        b"not admitted\n",
    )
    .unwrap();
    let before = snapshot_existing(&target);

    let plan = run(temp.path(), &["adopt", target.to_str().unwrap(), "--json"]);
    assert_eq!(snapshot_existing(&target), before);
    let operation_id = plan["data"]["operation_id"].as_str().unwrap();
    let applied = run(temp.path(), &["apply", operation_id, "--json"]);
    assert_eq!(applied["data"]["target"], target.to_str().unwrap());
    assert_eq!(snapshot_existing(&target), before);
    let status = run(
        temp.path(),
        &["status", "--vault", target.to_str().unwrap(), "--json"],
    );
    assert_eq!(status["data"]["admission"]["enabled"], 0);
}

fn snapshot_existing(root: &Path) -> BTreeMap<String, Vec<u8>> {
    ["Notes/keep.md", "Personal-disabled/private.txt"]
        .into_iter()
        .map(|relative| (relative.to_owned(), fs::read(root.join(relative)).unwrap()))
        .collect()
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
        "args={arguments:?}\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
