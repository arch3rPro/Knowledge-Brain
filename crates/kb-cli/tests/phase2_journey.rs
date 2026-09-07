use assert_cmd::Command;
use serde_json::Value;
use std::{
    collections::BTreeMap,
    fs,
    io::{Cursor, Write},
    path::Path,
};
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

#[test]
fn excluding_a_captured_file_does_not_mark_it_deleted() {
    let temporary = tempfile::tempdir().unwrap();
    let base = temporary.path();
    let v = base.join("vault");
    let p = v.to_str().unwrap();
    run(base, &["init", p, "--json"]);
    fs::create_dir(v.join("Notes")).unwrap();
    run(
        base,
        &[
            "config",
            "admission",
            "add",
            "notes",
            "Notes",
            "--vault",
            p,
            "--yes",
            "--json",
        ],
    );
    fs::write(v.join("Notes/a.md"), "source").unwrap();
    let r = run(base, &["review", "--vault", p, "--json"]);
    run(
        base,
        &[
            "apply",
            r["data"]["operation_id"].as_str().unwrap(),
            "--json",
        ],
    );
    fs::write(v.join("admission.yml"),"schema_version: v1.0\ndirectories:\n- id: notes\n  path: Notes\n  enabled: true\n  exclude: ['**/*.md']\n").unwrap();
    let r = run(base, &["review", "--vault", p, "--json"]);
    assert!(r["data"]["changes"].as_array().unwrap().is_empty());
}
#[test]
fn old_versions_remain_verifiable_after_record_edit_and_source_update() {
    let temporary = tempfile::tempdir().unwrap();
    let base = temporary.path();
    let v = base.join("vault");
    let p = v.to_str().unwrap();
    run(base, &["init", p, "--json"]);
    fs::create_dir(v.join("Notes")).unwrap();
    run(
        base,
        &[
            "config",
            "admission",
            "add",
            "notes",
            "Notes",
            "--vault",
            p,
            "--yes",
            "--json",
        ],
    );
    fs::write(v.join("Notes/a.md"), "source").unwrap();
    let r = run(base, &["review", "--vault", p, "--json"]);
    run(
        base,
        &[
            "apply",
            r["data"]["operation_id"].as_str().unwrap(),
            "--json",
        ],
    );
    let catalog = run(base, &["cache", "rebuild", "--vault", p, "--json"]);
    let record = catalog["data"]["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["scope"] == "sources")
        .unwrap()["path"]
        .as_str()
        .unwrap();
    let record = v.join(record);
    let md = fs::read_to_string(&record).unwrap().replacen(
        "type: Reference",
        "custom_field: keep-me\ntype: Reference",
        1,
    );
    fs::write(&record, format!("{md}\nHuman annotation.\n")).unwrap();
    fs::write(v.join("Notes/a.md"), "changed source").unwrap();
    let r = run(base, &["review", "--vault", p, "--json"]);
    run(
        base,
        &[
            "apply",
            r["data"]["operation_id"].as_str().unwrap(),
            "--json",
        ],
    );
    let query = run(
        base,
        &[
            "query",
            "Human annotation",
            "--scope",
            "sources",
            "--vault",
            p,
            "--json",
        ],
    );
    assert_eq!(
        query["data"]["groups"][0]["results"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let text = fs::read_to_string(record).unwrap();
    assert!(text.contains("custom_field: keep-me"));
    assert!(text.contains("Human annotation."));
    let verified = run(base, &["source", "verify", "--vault", p, "--json"]);
    assert_eq!(verified["data"]["checks"].as_array().unwrap().len(), 2);
    let object = v.join(verified["data"]["checks"][0]["path"].as_str().unwrap());
    fs::write(object, "corrupt").unwrap();
    let verified = run(base, &["source", "verify", "--vault", p, "--json"]);
    assert!(
        verified["data"]["checks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["status"] == "fail")
    );
}
#[test]
fn copied_vault_and_chinese_queries_work_without_machine_cache() {
    let temporary = tempfile::tempdir().unwrap();
    let base = temporary.path();
    let v = base.join("vault");
    let p = v.to_str().unwrap();
    let init = run(base, &["init", p, "--json"]);
    fs::write(
        v.join("Wiki/articles/manual.md"),
        "# 知识库\n本地检索无需模型\n",
    )
    .unwrap();
    let moved = base.join("moved");
    fs::rename(&v, &moved).unwrap();
    run(
        base,
        &[
            "vault",
            "rebind",
            init["data"]["vault_id"].as_str().unwrap(),
            moved.to_str().unwrap(),
            "--json",
        ],
    );
    let result = run(
        base,
        &[
            "query",
            "本地检索",
            "--vault",
            init["data"]["vault_id"].as_str().unwrap(),
            "--json",
        ],
    );
    assert_eq!(
        result["data"]["groups"][0]["results"][0]["path"],
        "Wiki/articles/manual.md"
    );
    let foreign = tempfile::tempdir().unwrap();
    let result = run(
        foreign.path(),
        &[
            "query",
            "本地检索",
            "--vault",
            moved.to_str().unwrap(),
            "--json",
        ],
    );
    assert_eq!(
        result["data"]["groups"][0]["results"][0]["path"],
        "Wiki/articles/manual.md"
    );
}
#[cfg(unix)]
#[test]
fn links_and_excluded_subtrees_do_not_contribute_source_text() {
    let temporary = tempfile::tempdir().unwrap();
    let base = temporary.path();
    let v = base.join("vault");
    let p = v.to_str().unwrap();
    run(base, &["init", p, "--json"]);
    fs::create_dir_all(v.join("Notes/tmp")).unwrap();
    run(
        base,
        &[
            "config",
            "admission",
            "add",
            "notes",
            "Notes",
            "--vault",
            p,
            "--yes",
            "--json",
        ],
    );
    fs::write(base.join("secret.md"), "secret").unwrap();
    std::os::unix::fs::symlink(base.join("secret.md"), v.join("Notes/link.md")).unwrap();
    fs::write(v.join("Notes/tmp/skip.md"), "excluded").unwrap();
    fs::write(v.join("admission.yml"),"schema_version: v1.0\ndirectories:\n- id: notes\n  path: Notes\n  enabled: true\n  exclude: ['tmp/**']\n").unwrap();
    let r = run(base, &["review", "--vault", p, "--json"]);
    assert!(r["data"]["changes"].as_array().unwrap().is_empty());
    assert!(
        r["data"]["skipped"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s["reason"] == "link")
    );
}
fn command(base: &Path) -> Command {
    let mut c = std::env::var_os("KB_TEST_BINARY")
        .map_or_else(|| Command::cargo_bin("kb").unwrap(), Command::new);
    c.env_clear();
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
            c.env(name, value);
        }
    }
    c.env("KB_CONFIG_DIR", base.join("config"))
        .env("KB_STATE_DIR", base.join("state"))
        .env("KB_CACHE_DIR", base.join("cache"));
    c
}
#[cfg(unix)]
#[test]
fn linked_runtime_is_rejected_before_external_files_are_changed() {
    let temporary = tempfile::tempdir().unwrap();
    let base = temporary.path();
    let vault = base.join("vault");
    let path = vault.to_str().unwrap();
    run(base, &["init", path, "--json"]);
    let external = base.join("external");
    fs::create_dir(&external).unwrap();
    fs::write(
        external.join("vault-lock-info.json"),
        "human-owned external data",
    )
    .unwrap();
    fs::rename(
        vault.join(".kb/runtime"),
        vault.join(".kb/runtime-original"),
    )
    .unwrap();
    std::os::unix::fs::symlink(&external, vault.join(".kb/runtime")).unwrap();
    assert_eq!(
        failed(base, &["cache", "rebuild", "--vault", path, "--json"])["error"]["code"],
        "unsafe_path"
    );
    assert_eq!(
        fs::read_to_string(external.join("vault-lock-info.json")).unwrap(),
        "human-owned external data"
    );
    assert!(!external.join("vault.lock").exists());
}
#[test]
fn admitted_git_root_is_still_excluded() {
    let temporary = tempfile::tempdir().unwrap();
    let base = temporary.path();
    let vault = base.join("vault");
    let path = vault.to_str().unwrap();
    run(base, &["init", path, "--json"]);
    fs::create_dir(vault.join(".git")).unwrap();
    fs::write(vault.join(".git/config"), "private repository metadata").unwrap();
    fs::write(vault.join("admission.yml"),"schema_version: v1.0\ndirectories:\n- id: git\n  path: .git\n  enabled: true\n  include: ['**/*']\n").unwrap();
    let result = failed(base, &["review", "--vault", path, "--json"]);
    assert_eq!(result["error"]["code"], "unsafe_path");
    assert_eq!(
        fs::read_to_string(vault.join(".git/config")).unwrap(),
        "private repository metadata"
    );
}
#[cfg(unix)]
#[test]
fn backslash_filename_is_rejected_instead_of_reinterpreted_as_a_path() {
    let temporary = tempfile::tempdir().unwrap();
    let base = temporary.path();
    let vault = base.join("vault");
    let path = vault.to_str().unwrap();
    run(base, &["init", path, "--json"]);
    fs::create_dir(vault.join("Notes")).unwrap();
    run(
        base,
        &[
            "config",
            "admission",
            "add",
            "notes",
            "Notes",
            "--vault",
            path,
            "--yes",
            "--json",
        ],
    );
    fs::write(vault.join("Notes/a\\b.md"), "needle").unwrap();
    assert_eq!(
        failed(base, &["review", "--vault", path, "--json"])["error"]["code"],
        "unsafe_path"
    );
}
#[test]
fn cache_write_failure_is_a_warning_not_a_failed_capture() {
    let temporary = tempfile::tempdir().unwrap();
    let base = temporary.path();
    let v = base.join("vault");
    let p = v.to_str().unwrap();
    run(base, &["init", p, "--json"]);
    fs::create_dir(v.join("Notes")).unwrap();
    run(
        base,
        &[
            "config",
            "admission",
            "add",
            "notes",
            "Notes",
            "--vault",
            p,
            "--yes",
            "--json",
        ],
    );
    fs::write(v.join("Notes/a.md"), "needle").unwrap();
    fs::write(
        v.join(".kb/cache/extracted"),
        "human file blocks cache creation",
    )
    .unwrap();
    let r = run(base, &["review", "--vault", p, "--json"]);
    let id = r["data"]["operation_id"].as_str().unwrap();
    let result = run(base, &["apply", id, "--json"]);
    assert!(!result["data"]["warnings"].as_array().unwrap().is_empty());
    assert_eq!(
        fs::read_to_string(v.join(".kb/cache/extracted")).unwrap(),
        "human file blocks cache creation"
    );
    assert_eq!(
        run(
            base,
            &[
                "query", "needle", "--scope", "sources", "--vault", p, "--json"
            ]
        )["data"]["groups"][0]["results"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}
#[test]
fn receipt_retry_clears_leftover_marker_and_unblocks_queries() {
    let temporary = tempfile::tempdir().unwrap();
    let base = temporary.path();
    let v = base.join("vault");
    let p = v.to_str().unwrap();
    run(base, &["init", p, "--json"]);
    fs::create_dir(v.join("Notes")).unwrap();
    run(
        base,
        &[
            "config",
            "admission",
            "add",
            "notes",
            "Notes",
            "--vault",
            p,
            "--yes",
            "--json",
        ],
    );
    fs::write(v.join("Notes/a.md"), "needle").unwrap();
    let r = run(base, &["review", "--vault", p, "--json"]);
    let id = r["data"]["operation_id"].as_str().unwrap();
    let first = run(base, &["apply", id, "--json"]);
    // Receipt persisted, process stopped before removing the recovery marker.
    fs::write(
        v.join(".kb/runtime/source-pending.json"),
        serde_json::to_vec(id).unwrap(),
    )
    .unwrap();
    assert_eq!(
        failed(base, &["query", "needle", "--vault", p, "--json"])["error"]["code"],
        "vault_needs_recovery"
    );
    assert_eq!(
        run(base, &["status", "--vault", p, "--json"])["data"]["recovery"]["pending_operations"],
        1
    );
    assert_eq!(
        failed(
            base,
            &[
                "config",
                "admission",
                "disable",
                "notes",
                "--vault",
                p,
                "--yes",
                "--json"
            ]
        )["error"]["code"],
        "vault_needs_recovery"
    );
    assert_eq!(run(base, &["apply", id, "--json"])["data"], first["data"]);
    assert!(!v.join(".kb/runtime/source-pending.json").exists());
    assert_eq!(
        run(
            base,
            &[
                "query", "needle", "--scope", "sources", "--vault", p, "--json"
            ]
        )["data"]["groups"][0]["results"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn unsupported_schema_source_apply_retry_preserves_vault_bytes() {
    let temporary = tempfile::tempdir().unwrap();
    let base = temporary.path();
    let vault = base.join("vault");
    let vault_text = vault.to_str().unwrap();
    run(base, &["init", vault_text, "--json"]);
    fs::create_dir(vault.join("Notes")).unwrap();
    run(
        base,
        &[
            "config",
            "admission",
            "add",
            "notes",
            "Notes",
            "--vault",
            vault_text,
            "--yes",
            "--json",
        ],
    );
    fs::write(vault.join("Notes/a.md"), "needle").unwrap();
    let review = run(base, &["review", "--vault", vault_text, "--json"]);
    let operation_id = review["data"]["operation_id"].as_str().unwrap();
    run(base, &["apply", operation_id, "--json"]);

    fs::write(vault.join(".kb/cache/catalog.json"), b"catalog sentinel").unwrap();
    fs::write(vault.join(".kb/cache/bm25.json"), b"bm25 sentinel").unwrap();
    fs::write(
        vault.join(".kb/runtime/source-pending.json"),
        serde_json::to_vec(operation_id).unwrap(),
    )
    .unwrap();
    let config_path = vault.join(".kb/config.yml");
    let unsupported_config = fs::read_to_string(&config_path)
        .unwrap()
        .replacen("v1.0", "v0.9", 1);
    fs::write(&config_path, unsupported_config).unwrap();
    let before = snapshot_vault(&vault);

    let error = failed(base, &["apply", operation_id, "--json"]);

    assert_eq!(error["error"]["code"], "migration_unavailable");
    assert_eq!(snapshot_vault(&vault), before);
}

#[test]
fn document_sources_survive_review_apply_query_and_reopening() {
    let temporary = tempfile::tempdir().unwrap();
    let base = temporary.path();
    let vault = base.join("vault");
    let path = vault.to_str().unwrap();
    run(base, &["init", path, "--json"]);
    fs::create_dir(vault.join("Documents")).unwrap();
    fs::write(
        vault.join("admission.yml"),
        "schema_version: v1.0\ndirectories:\n- id: documents\n  path: Documents\n  enabled: true\n  include: ['**/*.html', '**/*.docx']\n",
    )
    .unwrap();
    fs::write(
        vault.join("Documents/page.html"),
        "<html><head><title>Web Guide</title></head><body><h1>HTML Section</h1><p>cobalt browser source</p></body></html>",
    )
    .unwrap();
    fs::write(
        vault.join("Documents/guide.docx"),
        zip_bytes(&[
            (
                "docProps/core.xml",
                br#"<cp:coreProperties xmlns:cp="x" xmlns:dc="y"><dc:title>Office Guide</dc:title></cp:coreProperties>"#,
            ),
            (
                "word/document.xml",
                br#"<w:document xmlns:w="w"><w:body><w:p><w:r><w:t>amber office source</w:t></w:r></w:p></w:body></w:document>"#,
            ),
        ]),
    )
    .unwrap();

    let review = run(base, &["review", "--vault", path, "--json"]);
    let changes = review["data"]["changes"].as_array().unwrap();
    assert_eq!(changes.len(), 2);
    assert!(
        changes
            .iter()
            .all(|change| change["extraction"]["status"] == "text_ready")
    );
    let hashes = changes
        .iter()
        .map(|change| change["current_sha256"].as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    run(
        base,
        &[
            "apply",
            review["data"]["operation_id"].as_str().unwrap(),
            "--json",
        ],
    );

    let html = run(
        base,
        &[
            "query", "cobalt", "--scope", "sources", "--vault", path, "--json",
        ],
    );
    assert_eq!(
        html["data"]["groups"][0]["results"][0]["location"]["kind"],
        "html"
    );
    let docx = run(
        base,
        &[
            "query", "amber", "--scope", "sources", "--vault", path, "--json",
        ],
    );
    assert_eq!(
        docx["data"]["groups"][0]["results"][0]["location"]["kind"],
        "docx"
    );

    let cache_files = hashes
        .iter()
        .flat_map(|hash| {
            fs::read_dir(vault.join(format!(".kb/cache/extracted/{hash}")))
                .unwrap()
                .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        })
        .collect::<Vec<_>>();
    assert!(cache_files.contains(&"builtin-html-v1-html.json".to_owned()));
    assert!(cache_files.contains(&"builtin-docx-v1-docx.json".to_owned()));
}

fn zip_bytes(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    for (name, contents) in entries {
        writer.start_file(*name, options).unwrap();
        writer.write_all(contents).unwrap();
    }
    writer.finish().unwrap().into_inner()
}
fn run(base: &Path, args: &[&str]) -> Value {
    let o = command(base).args(args).output().unwrap();
    assert!(
        o.status.success(),
        "args={args:?}\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    );
    assert!(o.stderr.is_empty());
    serde_json::from_slice(&o.stdout).unwrap()
}
fn failed(base: &Path, args: &[&str]) -> Value {
    let o = command(base).args(args).output().unwrap();
    assert!(!o.status.success());
    assert!(o.stderr.is_empty());
    serde_json::from_slice(&o.stdout).unwrap()
}

fn snapshot_vault(root: &Path) -> BTreeMap<String, Vec<u8>> {
    fn visit(root: &Path, directory: &Path, files: &mut BTreeMap<String, Vec<u8>>) {
        for entry in fs::read_dir(directory).unwrap().map(Result::unwrap) {
            let path = entry.path();
            if path.is_dir() {
                visit(root, &path, files);
            } else {
                files.insert(
                    path.strip_prefix(root)
                        .unwrap()
                        .to_string_lossy()
                        .into_owned(),
                    fs::read(path).unwrap(),
                );
            }
        }
    }

    let mut files = BTreeMap::new();
    visit(root, root, &mut files);
    files
}
#[test]
// This single journey deliberately keeps sequential filesystem assertions together.
#[allow(clippy::too_many_lines)]
fn sources_survive_edits_moves_deletions_cache_loss_and_reopening() {
    let temporary = tempfile::tempdir().unwrap();
    let base = temporary.path();
    let v = base.join("vault");
    let p = v.to_str().unwrap();
    run(base, &["init", p, "--json"]);
    fs::create_dir(v.join("Notes")).unwrap();
    fs::create_dir(v.join("Private")).unwrap();
    run(
        base,
        &[
            "config",
            "admission",
            "add",
            "notes",
            "Notes",
            "--vault",
            p,
            "--yes",
            "--json",
        ],
    );
    fs::write(
        v.join("Notes/a.md"),
        "# Original\n唯一来源 sapphire evidence\n",
    )
    .unwrap();
    fs::write(v.join("Notes/.secret.md"), "classified sapphire").unwrap();
    fs::write(v.join("Private/no.md"), "classified sapphire").unwrap();
    let old_log = fs::read(v.join("Wiki/log.md")).unwrap();
    let review = run(base, &["review", "--vault", p, "--json"]);
    assert_eq!(review["data"]["changes"].as_array().unwrap().len(), 1);
    assert_eq!(review["data"]["changes"][0]["kind"], "added");
    assert_eq!(fs::read(v.join("Wiki/log.md")).unwrap(), old_log);
    let id = review["data"]["operation_id"].as_str().unwrap();
    let applied = run(base, &["apply", id, "--json"]);
    assert_eq!(run(base, &["apply", id, "--json"]), applied);
    assert_eq!(
        run(base, &["review", "--vault", p, "--json"])["data"]["operation_id"],
        Value::Null
    );
    fs::write(
        v.join("Wiki/articles/manual.md"),
        "# Sapphire\n人工知识 sapphire\n",
    )
    .unwrap();
    run(base, &["cache", "rebuild", "--vault", p, "--json"]);
    let all = run(
        base,
        &[
            "query", "sapphire", "--scope", "all", "--vault", p, "--json",
        ],
    );
    assert_eq!(all["data"]["groups"][0]["scope"], "wiki");
    assert_eq!(all["data"]["groups"][1]["scope"], "sources");
    assert_eq!(
        all["data"]["groups"][0]["results"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        all["data"]["groups"][1]["results"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    fs::remove_dir_all(v.join(".kb/cache")).unwrap();
    assert_eq!(
        run(
            base,
            &[
                "query", "sapphire", "--scope", "all", "--vault", p, "--json"
            ]
        ),
        all
    );
    fs::create_dir_all(v.join(".kb/cache")).unwrap();
    fs::write(v.join(".kb/cache/catalog.json"), "{corrupt").unwrap();
    assert_eq!(
        run(
            base,
            &[
                "query", "sapphire", "--scope", "all", "--vault", p, "--json"
            ]
        ),
        all
    );
    fs::rename(v.join("Notes/a.md"), v.join("Notes/base.md")).unwrap();
    let moved = run(base, &["review", "--vault", p, "--json"]);
    let changes = moved["data"]["changes"].as_array().unwrap();
    assert_eq!(changes.len(), 2);
    assert!(
        changes.iter().any(
            |c| c["kind"] == "added" && !c["possible_move_from"].as_array().unwrap().is_empty()
        )
    );
    run(
        base,
        &[
            "apply",
            moved["data"]["operation_id"].as_str().unwrap(),
            "--json",
        ],
    );
    fs::write(v.join("Notes/base.md"), "# Changed\nnew emerald evidence\n").unwrap();
    let modified = run(base, &["review", "--vault", p, "--json"]);
    assert_eq!(modified["data"]["changes"][0]["kind"], "modified");
    run(
        base,
        &[
            "apply",
            modified["data"]["operation_id"].as_str().unwrap(),
            "--json",
        ],
    );
    fs::remove_file(v.join("Notes/base.md")).unwrap();
    let deleted = run(base, &["review", "--vault", p, "--json"]);
    assert_eq!(deleted["data"]["changes"][0]["kind"], "deleted");
    run(
        base,
        &[
            "apply",
            deleted["data"]["operation_id"].as_str().unwrap(),
            "--json",
        ],
    );
    assert_eq!(
        run(base, &["review", "--vault", p, "--json"])["data"]["operation_id"],
        Value::Null
    );
    let verification = run(base, &["source", "verify", "--vault", p, "--json"]);
    assert!(
        verification["data"]["checks"]
            .as_array()
            .unwrap()
            .iter()
            .all(|c| c["status"] == "pass")
    );
    assert!(!v.join(".git").exists());
}
#[test]
fn stale_source_and_admission_plans_preserve_data() {
    let temporary = tempfile::tempdir().unwrap();
    let base = temporary.path();
    let v = base.join("vault");
    let p = v.to_str().unwrap();
    run(base, &["init", p, "--json"]);
    fs::create_dir(v.join("Notes")).unwrap();
    run(
        base,
        &[
            "config",
            "admission",
            "add",
            "notes",
            "Notes",
            "--vault",
            p,
            "--yes",
            "--json",
        ],
    );
    fs::write(v.join("Notes/a.md"), "old").unwrap();
    let review = run(base, &["review", "--vault", p, "--json"]);
    fs::write(v.join("Notes/a.md"), "new").unwrap();
    assert_eq!(
        failed(
            base,
            &[
                "apply",
                review["data"]["operation_id"].as_str().unwrap(),
                "--json"
            ]
        )["error"]["code"],
        "plan_stale"
    );
    let review = run(base, &["review", "--vault", p, "--json"]);
    run(
        base,
        &[
            "config",
            "admission",
            "disable",
            "notes",
            "--vault",
            p,
            "--yes",
            "--json",
        ],
    );
    assert_eq!(
        failed(
            base,
            &[
                "apply",
                review["data"]["operation_id"].as_str().unwrap(),
                "--json"
            ]
        )["error"]["code"],
        "plan_stale"
    );
    assert_eq!(fs::read(v.join("Notes/a.md")).unwrap(), b"new");
    assert!(!v.join("Wiki/external-sources/records").exists());
}
#[test]
fn invalid_query_and_limits_have_machine_errors() {
    let temporary = tempfile::tempdir().unwrap();
    let base = temporary.path();
    let v = base.join("vault");
    let p = v.to_str().unwrap();
    run(base, &["init", p, "--json"]);
    assert_eq!(
        failed(base, &["query", " ", "--vault", p, "--json"])["error"]["code"],
        "invalid_query"
    );
    fs::create_dir(v.join("Notes")).unwrap();
    run(
        base,
        &[
            "config",
            "admission",
            "add",
            "notes",
            "Notes",
            "--vault",
            p,
            "--yes",
            "--json",
        ],
    );
    run(
        base,
        &[
            "config",
            "set",
            "limits.max_file_bytes",
            "3",
            "--vault",
            p,
            "--yes",
            "--json",
        ],
    );
    fs::write(v.join("Notes/a.md"), "too large").unwrap();
    assert_eq!(
        failed(base, &["review", "--vault", p, "--json"])["error"]["code"],
        "limit_exceeded"
    );
}

#[test]
fn discovery_enforces_aggregate_and_entry_limits() {
    for (key, value) in [
        ("limits.max_total_read_bytes", "5"),
        ("limits.max_files_per_review", "1"),
    ] {
        let temporary = tempfile::tempdir().unwrap();
        let base = temporary.path();
        let vault = base.join("vault");
        let path = vault.to_str().unwrap();
        run(base, &["init", path, "--json"]);
        fs::create_dir(vault.join("Notes")).unwrap();
        run(
            base,
            &[
                "config",
                "admission",
                "add",
                "notes",
                "Notes",
                "--vault",
                path,
                "--yes",
                "--json",
            ],
        );
        run(
            base,
            &[
                "config", "set", key, value, "--vault", path, "--yes", "--json",
            ],
        );
        fs::write(vault.join("Notes/a.md"), "1234").unwrap();
        fs::write(vault.join("Notes/b.md"), "5678").unwrap();
        let error = failed(base, &["review", "--vault", path, "--json"]);
        assert_eq!(error["error"]["code"], "limit_exceeded");
        assert_eq!(error["error"]["details"]["limit"], key);
        assert!(!vault.join("Wiki/external-sources/records").exists());
    }
}

#[test]
fn direct_search_is_section_scoped_and_bm25_fallback_is_explicit() {
    let temporary = tempfile::tempdir().unwrap();
    let base = temporary.path();
    let vault = base.join("vault");
    let path = vault.to_str().unwrap();
    run(base, &["init", path, "--json"]);
    fs::write(
        vault.join("Wiki/articles/a.md"),
        "# Exact\nalpha beta alpha beta\n# Unrelated\nprivate neighbor\n",
    )
    .unwrap();
    fs::write(
        vault.join("Wiki/articles/b.md"),
        "# Terms\nalpha then beta\n",
    )
    .unwrap();
    run(
        base,
        &[
            "config",
            "set",
            "search.mode",
            "bm25",
            "--vault",
            path,
            "--yes",
            "--json",
        ],
    );
    let result = run(
        base,
        &[
            "query",
            "alpha beta",
            "--scope",
            "all",
            "--vault",
            path,
            "--json",
        ],
    );
    assert_eq!(
        result["data"]["groups"][0]["results"][0]["path"],
        "Wiki/articles/a.md"
    );
    assert_eq!(result["data"]["groups"][0]["results"][0]["line_start"], 1);
    assert_eq!(
        result["data"]["groups"][0]["results"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert!(
        !result["data"]["groups"][0]["results"][0]["snippet"]
            .as_str()
            .unwrap()
            .contains("private neighbor")
    );
    assert!(
        result["data"]["groups"][1]["results"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(!result["data"]["warnings"].as_array().unwrap().is_empty());
    run(base, &["cache", "rebuild", "--vault", path, "--json"]);
    assert!(
        !fs::read_to_string(vault.join(".kb/cache/catalog.json"))
            .unwrap()
            .contains("private neighbor")
    );
    fs::write(vault.join("Wiki/articles/a.md"), "# Changed\n即时修改\n").unwrap();
    assert_eq!(run(base,&["query","即时修改","--vault",path,"--json"])["data"]["groups"][0]["results"].as_array().unwrap().len(),1);
}
