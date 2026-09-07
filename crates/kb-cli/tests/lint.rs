use std::{collections::BTreeMap, fs, path::Path};

use assert_cmd::Command;

#[test]
fn lint_default_and_strict_share_report_but_use_different_exit_policy() {
    let temporary = tempfile::tempdir().unwrap();
    let vault = temporary.path().join("vault");
    run(
        temporary.path(),
        &["init", vault.to_str().unwrap(), "--json"],
    );
    fs::write(
        vault.join("Wiki/articles/unlinked.md"),
        "---\ntype: Article\n---\n",
    )
    .unwrap();

    let default = command(temporary.path())
        .args(["lint", "--vault", vault.to_str().unwrap(), "--json"])
        .output()
        .unwrap();
    assert!(default.status.success());
    assert!(default.stderr.is_empty());
    let default_json: serde_json::Value = serde_json::from_slice(&default.stdout).unwrap();
    assert_eq!(default_json["data"]["findings"][0]["code"], "orphan_concept");

    let strict = command(temporary.path())
        .args([
            "lint",
            "--strict",
            "--vault",
            vault.to_str().unwrap(),
            "--json",
        ])
        .output()
        .unwrap();
    assert!(!strict.status.success());
    assert!(strict.stderr.is_empty());
    let strict_json: serde_json::Value = serde_json::from_slice(&strict.stdout).unwrap();
    assert_eq!(strict_json["data"], default_json["data"]);
}

#[test]
fn strict_lint_succeeds_for_a_clean_minimum_vault() {
    let temporary = tempfile::tempdir().unwrap();
    let vault = temporary.path().join("vault");
    run(
        temporary.path(),
        &["init", vault.to_str().unwrap(), "--json"],
    );

    let output = command(temporary.path())
        .args([
            "lint",
            "--strict",
            "--vault",
            vault.to_str().unwrap(),
            "--json",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let response: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(response["data"]["findings"], serde_json::json!([]));
}

#[test]
fn lint_does_not_change_wiki_cache_or_knowledge_log() {
    let temporary = tempfile::tempdir().unwrap();
    let vault = temporary.path().join("vault");
    run(
        temporary.path(),
        &["init", vault.to_str().unwrap(), "--json"],
    );
    fs::write(
        vault.join("Wiki/articles/unlinked.md"),
        "---\ntype: Article\n---\n",
    )
    .unwrap();
    let before_wiki = snapshot(&vault.join("Wiki"));
    let before_cache = snapshot(&vault.join(".kb/cache"));

    run(
        temporary.path(),
        &["lint", "--vault", vault.to_str().unwrap(), "--json"],
    );

    assert_eq!(snapshot(&vault.join("Wiki")), before_wiki);
    assert_eq!(snapshot(&vault.join(".kb/cache")), before_cache);
}

fn run(base: &Path, arguments: &[&str]) -> serde_json::Value {
    let output = command(base).args(arguments).output().unwrap();
    assert!(
        output.status.success(),
        "args={arguments:?}\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn command(base: &Path) -> Command {
    let mut command = Command::cargo_bin("kb").unwrap();
    command
        .env("KB_CONFIG_DIR", base.join("config"))
        .env("KB_STATE_DIR", base.join("state"))
        .env("KB_CACHE_DIR", base.join("cache"));
    command
}

fn snapshot(root: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut output = BTreeMap::new();
    visit(root, root, &mut output);
    output
}

fn visit(root: &Path, directory: &Path, output: &mut BTreeMap<String, Vec<u8>>) {
    if !directory.exists() {
        return;
    }
    let mut entries = fs::read_dir(directory)
        .unwrap()
        .map(Result::unwrap)
        .collect::<Vec<_>>();
    entries.sort_by_key(fs::DirEntry::file_name);
    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            visit(root, &path, output);
        } else {
            output.insert(
                path.strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/"),
                fs::read(path).unwrap(),
            );
        }
    }
}
