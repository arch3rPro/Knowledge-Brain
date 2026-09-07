use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use kb_app::{
    AppContext, AppRequest, ConfigOverrides, InitRequest, UserPaths, init_vault, lint,
    load_effective_config, run,
};
use sha2::{Digest, Sha256};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

fn setup() -> (
    tempfile::TempDir,
    std::path::PathBuf,
    kb_core::EffectiveConfig,
) {
    let temporary = tempfile::tempdir().unwrap();
    let vault = temporary.path().join("vault");
    init_vault(&InitRequest {
        target: vault.clone(),
    })
    .unwrap();
    let paths = UserPaths::new(
        temporary.path().join("config"),
        temporary.path().join("state"),
        temporary.path().join("cache"),
    );
    let config = load_effective_config(&vault, &paths, &ConfigOverrides::default()).unwrap();
    (temporary, vault, config)
}

fn now() -> OffsetDateTime {
    OffsetDateTime::parse("2026-09-07T12:00:00+08:00", &Rfc3339).unwrap()
}

fn codes(report: &kb_app::LintReport) -> Vec<&str> {
    report
        .findings
        .iter()
        .map(|finding| finding.code.as_str())
        .collect()
}

#[test]
fn lint_resolves_links_reports_index_drift_and_finds_orphans() {
    let (_temporary, vault, config) = setup();
    fs::write(
        vault.join("Wiki/index.md"),
        "# Index\n\n[Current](articles/current.md)\n[Missing](articles/missing.md)\n",
    )
    .unwrap();
    fs::write(
        vault.join("Wiki/articles/current.md"),
        "---\ntype: Article\n---\n\n[Research](../research/answer%20one.md#result)\n[External](https://example.com)\n",
    )
    .unwrap();
    fs::write(
        vault.join("Wiki/research/answer one.md"),
        "---\ntype: Research\n---\n\nAnswer\n",
    )
    .unwrap();
    fs::write(
        vault.join("Wiki/articles/orphan.md"),
        "---\ntype: Article\n---\n\nUnlinked\n",
    )
    .unwrap();

    let report = lint(&vault, &config, now()).unwrap();

    assert_eq!(report.checked_files, 5);
    assert_eq!(codes(&report), ["orphan_concept", "index_drift"]);
    assert_eq!(report.findings[0].path.as_str(), "Wiki/articles/orphan.md");
    assert_eq!(report.findings[1].path.as_str(), "Wiki/index.md");
}

#[test]
fn lint_rejects_escaping_links_and_invalid_supersedes_targets() {
    let (_temporary, vault, config) = setup();
    fs::write(
        vault.join("Wiki/articles/new.md"),
        "---\ntype: Article\nkb:\n  supersedes:\n    - articles/new.md\n    - index.md\n    - ../outside.md\n---\n\n[Escape](../../outside.md)\n[Broken](missing.md)\n",
    )
    .unwrap();

    let report = lint(&vault, &config, now()).unwrap();

    assert_eq!(
        codes(&report),
        [
            "supersedes_invalid_target",
            "supersedes_invalid_target",
            "supersedes_self",
            "orphan_concept",
            "link_outside_wiki",
            "broken_link",
        ]
    );
}

#[test]
fn application_request_returns_the_shared_lint_report() {
    let (temporary, vault, _config) = setup();
    fs::write(
        vault.join("Wiki/articles/unlinked.md"),
        "---\ntype: Article\n---\n",
    )
    .unwrap();
    let environment = BTreeMap::from([
        (
            "KB_CONFIG_DIR".to_owned(),
            temporary.path().join("app-config").display().to_string(),
        ),
        (
            "KB_STATE_DIR".to_owned(),
            temporary.path().join("app-state").display().to_string(),
        ),
        (
            "KB_CACHE_DIR".to_owned(),
            temporary.path().join("app-cache").display().to_string(),
        ),
    ]);
    let response = run(
        AppRequest::Lint {
            vault: Some(vault.display().to_string()),
        },
        &AppContext::new(environment, temporary.path().to_path_buf()),
    )
    .unwrap();

    assert_eq!(response["findings"][0]["code"], "orphan_concept");
}

#[test]
fn lint_matches_exact_source_history_and_warns_when_not_current() {
    let (_temporary, vault, config) = setup();
    let old = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let current = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    let missing = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";
    write_source_record(&vault, old, current);
    fs::write(
        vault.join("Wiki/articles/source-links.md"),
        format!(
            "---\ntype: Article\nsources:\n  - id: old\n    resource: kb-source://notes/a.md?sha256={old}\n  - id: missing\n    resource: kb-source://notes/a.md?sha256={missing}\n---\n\nBody\n"
        ),
    )
    .unwrap();

    let report = lint(&vault, &config, now()).unwrap();

    assert!(codes(&report).contains(&"source_version_outdated"));
    assert!(codes(&report).contains(&"source_version_missing"));
}

#[test]
fn lint_is_read_only_and_reports_non_utf8_markdown() {
    let (_temporary, vault, config) = setup();
    fs::write(vault.join("Wiki/articles/bad.md"), [0xff, 0xfe]).unwrap();
    let before = snapshot(&vault);

    let report = lint(&vault, &config, now()).unwrap();

    assert!(codes(&report).contains(&"markdown_not_utf8"));
    assert_eq!(snapshot(&vault), before);
}

#[test]
fn lint_reports_portable_collisions_when_the_filesystem_can_store_them() {
    let (_temporary, vault, config) = setup();
    fs::write(
        vault.join("Wiki/articles/Case.md"),
        "---\ntype: Article\n---\n",
    )
    .unwrap();
    fs::write(
        vault.join("Wiki/articles/case.md"),
        "---\ntype: Article\n---\n",
    )
    .unwrap();
    let distinct_entries = fs::read_dir(vault.join("Wiki/articles"))
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .eq_ignore_ascii_case("case.md")
        })
        .count();
    if distinct_entries < 2 {
        return;
    }

    let report = lint(&vault, &config, now()).unwrap();

    assert_eq!(
        codes(&report)
            .into_iter()
            .filter(|code| *code == "portable_path_collision")
            .count(),
        2
    );
}

#[test]
fn source_record_current_version_must_appear_in_history() {
    let (_temporary, vault, config) = setup();
    let old = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let current = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    let path = write_source_record(&vault, old, current);
    let text = fs::read_to_string(&path).unwrap();
    let current_history = format!(
        "      - source:\n          admission_id: notes\n          relative_path: a.md\n        sha256: {current}\n"
    );
    fs::write(path, text.replacen(&current_history, "", 1)).unwrap();

    let report = lint(&vault, &config, now()).unwrap();

    assert!(codes(&report).contains(&"source_identity_invalid"));
}

fn write_source_record(vault: &Path, old: &str, current: &str) -> PathBuf {
    let logical = "kb-source://notes/a.md";
    let record_digest = hex::encode(Sha256::digest(logical.as_bytes()));
    let path = vault.join(format!(
        "Wiki/external-sources/records/{}/{}.md",
        &record_digest[..2],
        record_digest
    ));
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(
        &path,
        format!(
            "---\ntype: Reference\ntitle: a.md\nkb:\n  source:\n    source:\n      source:\n        admission_id: notes\n        relative_path: a.md\n      sha256: {current}\n    title: a.md\n    size: 1\n    media_type: markdown\n    extraction_status: text_ready\n    present: true\n    captured_at: 2026-09-07T03:00:00Z\n    versions:\n      - source:\n          admission_id: notes\n          relative_path: a.md\n        sha256: {old}\n      - source:\n          admission_id: notes\n          relative_path: a.md\n        sha256: {current}\n---\n\n# a.md\n"
        ),
    )
    .unwrap();
    path
}

fn snapshot(root: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut output = BTreeMap::new();
    visit(root, root, &mut output);
    output
}

fn visit(root: &Path, directory: &Path, output: &mut BTreeMap<String, Vec<u8>>) {
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
