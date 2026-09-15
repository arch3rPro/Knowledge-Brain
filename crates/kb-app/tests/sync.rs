use std::{collections::BTreeMap, fs, process::Command};

use kb_app::{
    ConfigOverrides, InitRequest, UserPaths, check_sync, ensure_shared_write_sync_safe, init_vault,
    register_vault,
};

fn user_paths(root: &std::path::Path) -> UserPaths {
    UserPaths::new(root.join("config"), root.join("state"), root.join("cache"))
}

#[test]
fn path_independent_vault_identity_keeps_absolute_paths_local() {
    let temporary = tempfile::tempdir().unwrap();
    let first = temporary.path().join("device one").join("Knowledge Vault");
    let second = temporary.path().join("device-two").join("vault");
    fs::create_dir_all(first.parent().unwrap()).unwrap();
    fs::create_dir_all(second.parent().unwrap()).unwrap();
    let initialized = init_vault(&InitRequest {
        target: first.clone(),
    })
    .unwrap();
    init_vault(&InitRequest {
        target: second.clone(),
    })
    .unwrap();
    fs::copy(first.join(".kb/config.yml"), second.join(".kb/config.yml")).unwrap();

    let first_user = user_paths(&temporary.path().join("first-user"));
    let second_user = user_paths(&temporary.path().join("second-user"));
    register_vault(&first_user, &first).unwrap();
    register_vault(&second_user, &second).unwrap();

    let first_report = check_sync(&first, &first_user, &ConfigOverrides::default()).unwrap();
    let second_report = check_sync(&second, &second_user, &ConfigOverrides::default()).unwrap();

    assert_eq!(first_report.vault_id, initialized.vault_id);
    assert_eq!(second_report.vault_id, initialized.vault_id);
    assert_eq!(first_report.root, first);
    assert_eq!(second_report.root, second);
    assert!(
        !fs::read_to_string(first.join(".kb/config.yml"))
            .unwrap()
            .contains(first.to_str().unwrap())
    );
    assert!(
        !fs::read_to_string(second.join(".kb/config.yml"))
            .unwrap()
            .contains(second.to_str().unwrap())
    );
}

#[test]
fn common_check_reports_conflict_markers_without_writing() {
    let temporary = tempfile::tempdir().unwrap();
    let vault = temporary.path().join("vault");
    init_vault(&InitRequest {
        target: vault.clone(),
    })
    .unwrap();
    fs::write(
        vault.join("Wiki/index.md"),
        "# Knowledge index\n\n<<<<<<< ours\n=======\n>>>>>>> theirs\n",
    )
    .unwrap();
    let before = fs::read(vault.join("Wiki/index.md")).unwrap();

    let report = check_sync(
        &vault,
        &user_paths(&temporary.path().join("user")),
        &ConfigOverrides::default(),
    )
    .unwrap();

    assert!(report.summary.writes_blocked);
    assert!(
        report
            .findings
            .iter()
            .any(|finding| finding.code == "sync_conflict_marker" && finding.blocks_writes)
    );
    assert_eq!(fs::read(vault.join("Wiki/index.md")).unwrap(), before);
    assert!(!vault.join(".kb/runtime/vault.lock").exists());
}

#[test]
fn non_git_vault_is_valid_and_git_is_optional() {
    let temporary = tempfile::tempdir().unwrap();
    let vault = temporary.path().join("vault");
    init_vault(&InitRequest {
        target: vault.clone(),
    })
    .unwrap();

    let report = check_sync(
        &vault,
        &user_paths(&temporary.path().join("user")),
        &ConfigOverrides {
            environment: BTreeMap::new(),
            cli: BTreeMap::new(),
        },
    )
    .unwrap();

    assert_eq!(format!("{:?}", report.git.state), "NotRepository");
    assert!(!report.summary.writes_blocked);
}

#[test]
fn fetched_upstream_commits_block_shared_writes_until_the_vault_is_updated() {
    if Command::new("git").arg("--version").output().is_err() {
        return;
    }
    let temporary = tempfile::tempdir().unwrap();
    let remote = temporary.path().join("remote.git");
    fs::create_dir(&remote).unwrap();
    git(&remote, &["init", "--bare"]);

    let vault = temporary.path().join("vault");
    init_vault(&InitRequest {
        target: vault.clone(),
    })
    .unwrap();
    git(&vault, &["init", "-b", "main"]);
    git(&vault, &["config", "user.name", "Sync Test"]);
    git(&vault, &["config", "user.email", "sync@example.invalid"]);
    git(&vault, &["add", "."]);
    git(&vault, &["commit", "-m", "initial"]);
    git(
        &vault,
        &["remote", "add", "origin", remote.to_str().unwrap()],
    );
    git(&vault, &["push", "-u", "origin", "main"]);

    let other = temporary.path().join("other");
    let clone = Command::new("git")
        .args(["clone", remote.to_str().unwrap(), other.to_str().unwrap()])
        .status()
        .unwrap();
    assert!(clone.success());
    git(&other, &["config", "user.name", "Other Writer"]);
    git(&other, &["config", "user.email", "other@example.invalid"]);
    fs::write(other.join("remote.md"), "remote change\n").unwrap();
    git(&other, &["add", "remote.md"]);
    git(&other, &["commit", "-m", "remote change"]);
    git(&other, &["push", "origin", "main"]);
    git(&vault, &["fetch", "origin"]);

    let report = check_sync(
        &vault,
        &user_paths(&temporary.path().join("user")),
        &ConfigOverrides::default(),
    )
    .unwrap();

    assert!(report.findings.iter().any(|finding| {
        finding.code == "git_branch_behind"
            && finding.level == kb_app::SyncFindingLevel::Error
            && finding.blocks_writes
    }));
    let error = ensure_shared_write_sync_safe(&vault).unwrap_err();
    assert_eq!(error.code, kb_core::ErrorCode::SyncConflict);

    fs::write(vault.join("local.md"), "local change\n").unwrap();
    git(&vault, &["add", "local.md"]);
    git(&vault, &["commit", "-m", "local change"]);

    let diverged = check_sync(
        &vault,
        &user_paths(&temporary.path().join("diverged-user")),
        &ConfigOverrides::default(),
    )
    .unwrap();
    assert!(diverged.findings.iter().any(|finding| {
        finding.code == "git_branch_diverged"
            && finding.level == kb_app::SyncFindingLevel::Error
            && finding.blocks_writes
    }));
}

// Windows cannot create this fixture because `CON` is a reserved device name.
// The platform-independent parser rule is covered in kb-core/tests/path_rules.rs.
#[cfg(not(windows))]
#[test]
fn common_check_reports_windows_reserved_shared_paths() {
    let temporary = tempfile::tempdir().unwrap();
    let vault = temporary.path().join("vault");
    init_vault(&InitRequest {
        target: vault.clone(),
    })
    .unwrap();
    fs::create_dir(vault.join("CON")).unwrap();
    fs::write(vault.join("CON/note.md"), "# Portable risk\n").unwrap();

    let report = check_sync(
        &vault,
        &user_paths(&temporary.path().join("user")),
        &ConfigOverrides::default(),
    )
    .unwrap();

    assert!(
        report
            .findings
            .iter()
            .any(|finding| finding.code == "portable_path_unsafe"
                && finding.path.as_deref() == Some("CON"))
    );
}

#[test]
fn duplicate_wiki_titles_block_managed_publication() {
    let temporary = tempfile::tempdir().unwrap();
    let vault = temporary.path().join("vault");
    init_vault(&InitRequest {
        target: vault.clone(),
    })
    .unwrap();
    fs::write(
        vault.join("Wiki/research/one.md"),
        "---\ntype: Research\n---\n\n# Same concept\n",
    )
    .unwrap();
    fs::write(
        vault.join("Wiki/research/two.md"),
        "---\ntype: Research\n---\n\n# Same concept\n",
    )
    .unwrap();

    let report = check_sync(
        &vault,
        &user_paths(&temporary.path().join("user")),
        &ConfigOverrides::default(),
    )
    .unwrap();

    assert!(report.findings.iter().any(|finding| {
        finding.code == "duplicate_title"
            && finding.level == kb_app::SyncFindingLevel::Error
            && finding.blocks_writes
    }));
}

#[test]
fn synchronized_content_change_reports_local_bm25_as_stale() {
    let temporary = tempfile::tempdir().unwrap();
    let vault = temporary.path().join("vault");
    init_vault(&InitRequest {
        target: vault.clone(),
    })
    .unwrap();
    let config_path = vault.join(".kb/config.yml");
    let config_text = fs::read_to_string(&config_path)
        .unwrap()
        .replace("mode: direct", "mode: bm25");
    fs::write(&config_path, config_text).unwrap();
    let paths = user_paths(&temporary.path().join("user"));
    let config =
        kb_app::load_effective_config(&vault, &paths, &ConfigOverrides::default()).unwrap();
    kb_app::rebuild_catalog(&vault, &config).unwrap();
    fs::write(
        vault.join("Wiki/articles/changed.md"),
        "---\ntype: Article\n---\n\n# Changed elsewhere\n",
    )
    .unwrap();

    let report = check_sync(&vault, &paths, &ConfigOverrides::default()).unwrap();

    assert!(report.findings.iter().any(|finding| {
        finding.code == "local_search_index_stale"
            && finding.level == kb_app::SyncFindingLevel::Warning
            && !finding.blocks_writes
    }));
}

#[test]
fn git_check_reports_tracked_local_state_and_missing_portable_rules() {
    if Command::new("git").arg("--version").output().is_err() {
        return;
    }
    let temporary = tempfile::tempdir().unwrap();
    let vault = temporary.path().join("vault with spaces");
    init_vault(&InitRequest {
        target: vault.clone(),
    })
    .unwrap();
    git(&vault, &["init"]);
    git(&vault, &["config", "user.name", "Sync Test"]);
    git(&vault, &["config", "user.email", "sync@example.invalid"]);
    fs::write(vault.join(".kb/cache/bm25.json"), "local index").unwrap();
    git(&vault, &["add", "."]);

    let report = check_sync(
        &vault,
        &user_paths(&temporary.path().join("user")),
        &ConfigOverrides::default(),
    )
    .unwrap();

    assert_eq!(format!("{:?}", report.git.state), "Checked");
    assert!(
        report
            .findings
            .iter()
            .any(|finding| finding.code == "local_state_tracked"
                && finding.path.as_deref() == Some(".kb/cache/bm25.json"))
    );
    assert!(
        report
            .findings
            .iter()
            .any(|finding| finding.code == "gitignore_missing_portable_rule")
    );
    assert!(
        report
            .findings
            .iter()
            .any(|finding| finding.code == "gitattributes_text_unstable")
    );
    assert!(
        report
            .findings
            .iter()
            .any(|finding| finding.code == "gitattributes_source_object_rewrite")
    );
}

#[test]
fn git_check_reports_an_unresolved_merge_without_resolving_it() {
    if Command::new("git").arg("--version").output().is_err() {
        return;
    }
    let temporary = tempfile::tempdir().unwrap();
    let vault = temporary.path().join("vault");
    init_vault(&InitRequest {
        target: vault.clone(),
    })
    .unwrap();
    git(&vault, &["init", "-b", "main"]);
    git(&vault, &["config", "user.name", "Sync Test"]);
    git(&vault, &["config", "user.email", "sync@example.invalid"]);
    fs::write(vault.join("shared.md"), "base\n").unwrap();
    git(&vault, &["add", "."]);
    git(&vault, &["commit", "-m", "base"]);
    git(&vault, &["checkout", "-b", "other"]);
    fs::write(vault.join("shared.md"), "other\n").unwrap();
    git(&vault, &["commit", "-am", "other"]);
    git(&vault, &["checkout", "main"]);
    fs::write(vault.join("shared.md"), "main\n").unwrap();
    git(&vault, &["commit", "-am", "main"]);
    let merge = Command::new("git")
        .arg("-C")
        .arg(&vault)
        .args(["merge", "other"])
        .output()
        .unwrap();
    assert!(!merge.status.success());

    let before = fs::read(vault.join(".git/index")).unwrap();
    let report = check_sync(
        &vault,
        &user_paths(&temporary.path().join("user")),
        &ConfigOverrides::default(),
    )
    .unwrap();

    assert!(report.findings.iter().any(|finding| {
        finding.code == "git_unmerged"
            && finding.path.as_deref() == Some("shared.md")
            && finding.blocks_writes
    }));
    assert_eq!(fs::read(vault.join(".git/index")).unwrap(), before);
}

fn git(root: &std::path::Path, arguments: &[&str]) {
    let status = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(arguments)
        .status()
        .unwrap();
    assert!(status.success(), "git {arguments:?}");
}
