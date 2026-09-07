use std::{collections::BTreeMap, fs, path::Path};

use kb_app::{
    AppContext, AppRequest, ConfigOverrides, InitRequest, UserPaths, apply_knowledge,
    create_knowledge_plan, init_vault, load_effective_config, run,
};
use kb_core::{
    CURRENT_SCHEMA_VERSION, ErrorCode, KnowledgeChangeRequest, KnowledgePlanRequest,
    PortableRelativePath,
};
use sha2::{Digest, Sha256};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

fn setup() -> (
    tempfile::TempDir,
    std::path::PathBuf,
    UserPaths,
    kb_core::KnowledgePlan,
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
    let request = KnowledgePlanRequest {
        schema_version: CURRENT_SCHEMA_VERSION,
        changes: vec![KnowledgeChangeRequest {
            path: PortableRelativePath::parse("articles/saved.md").unwrap(),
            before_sha256: None,
            summary: "Save the managed article.".into(),
            content: "---\ntype: Article\ntitle: Saved\nstatus: stable\ngenerated:\n  by: process:test\n  at: 2026-09-07T03:00:00Z\nsources:\n  - id: source\n    resource: https://example.com\nkb:\n  managed: true\n---\n\n# Saved\nDurable content.\n".into(),
        }],
    };
    let now = OffsetDateTime::parse("2026-09-07T12:00:00+08:00", &Rfc3339).unwrap();
    let plan = create_knowledge_plan(&vault, &paths, &config, request, now).unwrap();
    (temporary, vault, paths, plan)
}

#[test]
fn apply_saves_complete_plan_and_repeated_id_returns_same_result() {
    let (_temporary, vault, paths, plan) = setup();

    let first = apply_knowledge(&paths, plan.operation_id, &ConfigOverrides::default()).unwrap();
    let after_first = snapshot(&vault.join("Wiki"));
    let second = apply_knowledge(&paths, plan.operation_id, &ConfigOverrides::default()).unwrap();

    assert_eq!(second, first);
    assert_eq!(snapshot(&vault.join("Wiki")), after_first);
    assert!(
        fs::read_to_string(vault.join("Wiki/articles/saved.md"))
            .unwrap()
            .contains("Durable content.")
    );
    assert!(
        fs::read_to_string(vault.join("Wiki/index.md"))
            .unwrap()
            .contains("[Saved](articles/saved.md)")
    );
    let log = fs::read_to_string(vault.join("Wiki/log.md")).unwrap();
    assert_eq!(log.matches("Save the managed article.").count(), 1);
    assert!(!vault.join(".kb/runtime/knowledge-pending.json").exists());
}

#[test]
fn stale_target_rejects_every_plan_write() {
    let (_temporary, vault, paths, plan) = setup();
    fs::write(
        vault.join("Wiki/articles/saved.md"),
        "human created this after planning",
    )
    .unwrap();
    let before = snapshot(&vault.join("Wiki"));

    let error =
        apply_knowledge(&paths, plan.operation_id, &ConfigOverrides::default()).unwrap_err();

    assert_eq!(error.code, ErrorCode::PlanStale);
    assert_eq!(snapshot(&vault.join("Wiki")), before);
    assert!(!vault.join(".kb/runtime/knowledge-pending.json").exists());
}

#[test]
fn source_and_knowledge_pending_states_are_mutually_exclusive() {
    let (_temporary, vault, paths, plan) = setup();
    fs::write(
        vault.join(".kb/runtime/source-pending.json"),
        serde_json::to_vec(&plan.operation_id).unwrap(),
    )
    .unwrap();

    let error =
        apply_knowledge(&paths, plan.operation_id, &ConfigOverrides::default()).unwrap_err();

    assert_eq!(error.code, ErrorCode::VaultNeedsRecovery);
    assert!(!vault.join("Wiki/articles/saved.md").exists());
}

#[test]
fn status_counts_interrupted_knowledge_save() {
    let (temporary, vault, _paths, plan) = setup();
    fs::write(
        vault.join(".kb/runtime/knowledge-pending.json"),
        serde_json::to_vec(&plan.operation_id).unwrap(),
    )
    .unwrap();
    let context = AppContext::new(BTreeMap::new(), temporary.path().to_path_buf());
    let report = run(
        AppRequest::Status {
            vault: Some(vault.display().to_string()),
        },
        &context,
    )
    .unwrap();
    assert_eq!(report["recovery"]["pending_operations"], 1);
}

#[test]
fn tampered_expired_and_missing_source_plans_are_rejected() {
    let (_temporary, _vault, paths, plan) = setup();
    let directory = operation_directory(&paths, &plan);
    let plan_path = directory.join("plan.json");
    let mut value: serde_json::Value =
        serde_json::from_slice(&fs::read(&plan_path).unwrap()).unwrap();
    value["diff"] = "tampered".into();
    fs::write(&plan_path, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
    let error =
        apply_knowledge(&paths, plan.operation_id, &ConfigOverrides::default()).unwrap_err();
    assert_eq!(error.code, ErrorCode::PlanStale);

    let (_temporary, _vault, paths, mut plan) = setup();
    plan.created_at = "2000-01-01T00:00:00Z".into();
    rewrite_plan(&paths, &plan);
    let error =
        apply_knowledge(&paths, plan.operation_id, &ConfigOverrides::default()).unwrap_err();
    assert_eq!(error.code, ErrorCode::PlanStale);

    let (_temporary, _vault, paths, mut plan) = setup();
    plan.source_versions.push("kb-source://notes/missing.md?sha256=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into());
    rewrite_plan(&paths, &plan);
    let error =
        apply_knowledge(&paths, plan.operation_id, &ConfigOverrides::default()).unwrap_err();
    assert_eq!(error.code, ErrorCode::PlanStale);
}

#[test]
fn unrelated_files_survive_and_catalog_cleanup_failure_is_a_warning() {
    let (_temporary, vault, paths, plan) = setup();
    let unrelated = vault.join("Wiki/articles/human.md");
    fs::write(&unrelated, "human-owned bytes").unwrap();
    fs::create_dir(vault.join(".kb/cache/catalog.json")).unwrap();

    let first = apply_knowledge(&paths, plan.operation_id, &ConfigOverrides::default()).unwrap();
    let second = apply_knowledge(&paths, plan.operation_id, &ConfigOverrides::default()).unwrap();

    assert_eq!(fs::read_to_string(unrelated).unwrap(), "human-owned bytes");
    assert_eq!(first.warnings.len(), 1);
    assert_eq!(second.warnings, first.warnings);
}

fn operation_directory(paths: &UserPaths, plan: &kb_core::KnowledgePlan) -> std::path::PathBuf {
    paths
        .state_dir
        .join("operations")
        .join(plan.operation_id.to_string())
}

fn rewrite_plan(paths: &UserPaths, plan: &kb_core::KnowledgePlan) {
    let directory = operation_directory(paths, plan);
    fs::write(
        directory.join("plan.json"),
        serde_json::to_vec_pretty(plan).unwrap(),
    )
    .unwrap();
    let digest = hex::encode(Sha256::digest(serde_json::to_vec(plan).unwrap()));
    fs::write(
        directory.join("plan.sha256"),
        serde_json::to_vec_pretty(&digest).unwrap(),
    )
    .unwrap();
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
