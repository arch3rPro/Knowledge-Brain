use std::{collections::BTreeMap, fs, path::Path};

use assert_cmd::Command;

#[test]
fn adopt_reviews_then_applies_without_changing_existing_content() {
    let temp = tempfile::tempdir().unwrap();
    let target = temp.path().join("existing");
    fs::create_dir_all(target.join("Notes")).unwrap();
    let note = target.join("Notes/keep.md");
    fs::write(&note, b"human text\n").unwrap();
    let before = snapshot_tree(&target);

    let planned = run(temp.path(), &["adopt", target.to_str().unwrap(), "--json"]);
    let operation_id = planned["data"]["operation_id"].as_str().unwrap();
    assert_eq!(planned["data"]["kind"], "adopt_vault");
    assert_eq!(snapshot_tree(&target), before);

    let inspected = run(temp.path(), &["operation", "show", operation_id, "--json"]);
    assert_eq!(inspected["data"]["state"], "planned");

    let applied = run(temp.path(), &["apply", operation_id, "--json"]);
    assert_eq!(applied["data"]["operation_id"], operation_id);
    assert_eq!(fs::read(&note).unwrap(), b"human text\n");
    assert!(target.join(".kb/config.yml").is_file());
    assert!(!target.join(".git").exists());

    let repeated = run(temp.path(), &["apply", operation_id, "--json"]);
    assert_eq!(repeated, applied);
    let inspected = run(temp.path(), &["operation", "show", operation_id, "--json"]);
    assert_eq!(inspected["data"]["state"], "applied");
}

#[test]
fn stale_adoption_plan_fails_without_framework_changes() {
    let temp = tempfile::tempdir().unwrap();
    let target = temp.path().join("existing");
    fs::create_dir(&target).unwrap();
    let note = target.join("keep.md");
    fs::write(&note, b"before\n").unwrap();
    let planned = run(temp.path(), &["adopt", target.to_str().unwrap(), "--json"]);
    fs::write(&note, b"after review\n").unwrap();

    let failed = run_failure(
        temp.path(),
        &[
            "apply",
            planned["data"]["operation_id"].as_str().unwrap(),
            "--json",
        ],
    );

    assert_eq!(failed["error"]["code"], "plan_stale");
    assert_eq!(fs::read(&note).unwrap(), b"after review\n");
    assert!(!target.join(".kb").exists());
    assert!(!target.join("admission.yml").exists());
}

fn run(user_root: &Path, arguments: &[&str]) -> serde_json::Value {
    let output = command(user_root).args(arguments).output().unwrap();
    assert!(
        output.status.success(),
        "args={arguments:?}\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn run_failure(user_root: &Path, arguments: &[&str]) -> serde_json::Value {
    let output = command(user_root).args(arguments).output().unwrap();
    assert!(!output.status.success());
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

fn snapshot_tree(root: &Path) -> BTreeMap<String, Vec<u8>> {
    fn visit(root: &Path, path: &Path, output: &mut BTreeMap<String, Vec<u8>>) {
        let mut entries = fs::read_dir(path)
            .unwrap()
            .map(|entry| entry.unwrap())
            .collect::<Vec<_>>();
        entries.sort_by_key(std::fs::DirEntry::file_name);
        for entry in entries {
            let path = entry.path();
            let relative = path
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            if path.is_dir() {
                output.insert(format!("{relative}/"), Vec::new());
                visit(root, &path, output);
            } else {
                output.insert(relative, fs::read(path).unwrap());
            }
        }
    }
    let mut output = BTreeMap::new();
    visit(root, root, &mut output);
    output
}
