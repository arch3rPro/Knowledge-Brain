use std::{collections::BTreeMap, fs, path::Path};

use kb_app::{AppContext, AppRequest, InitRequest, SaveMode, run};
use kb_core::{
    CURRENT_SCHEMA_VERSION, KnowledgeChangeRequest, KnowledgePlanRequest, OperationId,
    PortableRelativePath,
};

#[test]
fn source_save_confirms_the_prepared_capture_without_exposing_an_apply_step() {
    let temporary = tempfile::tempdir().unwrap();
    let context = context(temporary.path());
    let vault = initialized_vault(&context, temporary.path());
    let vault_text = vault.display().to_string();
    add_admitted_note(&vault);

    let prepared = run(
        AppRequest::SourceSave {
            vault: Some(vault_text.clone()),
            mode: SaveMode::Prepare,
        },
        &context,
    )
    .unwrap();

    assert_eq!(prepared["phase"], "awaiting_confirmation");
    assert_eq!(prepared["change_summary"]["change_count"], 1);
    assert_eq!(
        prepared["change_summary"]["affected_paths"],
        serde_json::json!(["Notes/one.md"])
    );
    let token: OperationId = prepared["confirmation_token"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let before_confirmation = run(
        AppRequest::SourceVerify {
            vault: Some(vault_text.clone()),
        },
        &context,
    )
    .unwrap();
    assert!(before_confirmation["checks"].as_array().unwrap().is_empty());

    let applied = run(
        AppRequest::SourceSave {
            vault: Some(vault_text.clone()),
            mode: SaveMode::Confirm(token),
        },
        &context,
    )
    .unwrap();

    assert_eq!(applied["phase"], "applied");
    assert!(applied["confirmation_token"].is_null());
    assert_eq!(applied["change_summary"]["change_count"], 1);
    assert!(applied["result"].is_object());
    let verified = run(
        AppRequest::SourceVerify {
            vault: Some(vault_text),
        },
        &context,
    )
    .unwrap();
    assert_eq!(verified["checks"][0]["status"], "pass");
}

#[test]
fn source_save_reports_unchanged_after_the_confirmed_capture() {
    let temporary = tempfile::tempdir().unwrap();
    let context = context(temporary.path());
    let vault = initialized_vault(&context, temporary.path());
    let vault_text = vault.display().to_string();
    add_admitted_note(&vault);

    let prepared = run(
        AppRequest::SourceSave {
            vault: Some(vault_text.clone()),
            mode: SaveMode::Prepare,
        },
        &context,
    )
    .unwrap();
    let token = prepared["confirmation_token"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    run(
        AppRequest::SourceSave {
            vault: Some(vault_text.clone()),
            mode: SaveMode::Confirm(token),
        },
        &context,
    )
    .unwrap();

    let unchanged = run(
        AppRequest::SourceSave {
            vault: Some(vault_text),
            mode: SaveMode::Prepare,
        },
        &context,
    )
    .unwrap();

    assert_eq!(unchanged["phase"], "unchanged");
    assert!(unchanged["confirmation_token"].is_null());
    assert!(unchanged["result"].is_null());
}

#[test]
fn knowledge_save_confirms_the_prepared_wiki_change() {
    let temporary = tempfile::tempdir().unwrap();
    let context = context(temporary.path());
    let vault = initialized_vault(&context, temporary.path());
    let vault_text = vault.display().to_string();
    let article = vault.join("Wiki/articles/composite.md");

    let prepared = run(
        AppRequest::KnowledgeSave {
            vault: Some(vault_text.clone()),
            request: Some(knowledge_request()),
            mode: SaveMode::Prepare,
        },
        &context,
    )
    .unwrap();

    assert_eq!(prepared["phase"], "awaiting_confirmation");
    assert_eq!(prepared["change_summary"]["change_count"], 3);
    assert_eq!(
        prepared["change_summary"]["affected_paths"],
        serde_json::json!(["Wiki/articles/composite.md", "Wiki/index.md", "Wiki/log.md"])
    );
    assert!(!article.exists());
    let token = prepared["confirmation_token"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();

    let applied = run(
        AppRequest::KnowledgeSave {
            vault: Some(vault_text),
            request: None,
            mode: SaveMode::Confirm(token),
        },
        &context,
    )
    .unwrap();

    assert_eq!(applied["phase"], "applied");
    assert!(applied["confirmation_token"].is_null());
    assert!(applied["result"]["changed"].is_array());
    assert!(article.is_file());
}

#[test]
fn knowledge_save_can_prepare_and_apply_after_prior_authorization() {
    let temporary = tempfile::tempdir().unwrap();
    let context = context(temporary.path());
    let vault = initialized_vault(&context, temporary.path());
    let vault_text = vault.display().to_string();
    let article = vault.join("Wiki/articles/composite.md");

    let applied = run(
        AppRequest::KnowledgeSave {
            vault: Some(vault_text),
            request: Some(knowledge_request()),
            mode: SaveMode::ApplyImmediately,
        },
        &context,
    )
    .unwrap();

    assert_eq!(applied["phase"], "applied");
    assert!(applied["confirmation_token"].is_null());
    assert!(article.is_file());
}

fn initialized_vault(context: &AppContext, base: &Path) -> std::path::PathBuf {
    let vault = base.join("vault");
    run(
        AppRequest::Init(InitRequest {
            target: vault.clone(),
        }),
        context,
    )
    .unwrap();
    vault
}

fn add_admitted_note(vault: &Path) {
    fs::create_dir(vault.join("Notes")).unwrap();
    fs::write(vault.join("Notes/one.md"), "# One\n\nSource evidence.\n").unwrap();
    fs::write(
        vault.join("admission.yml"),
        "schema_version: v1.0\ndirectories:\n  - id: notes\n    path: Notes\n    enabled: true\n",
    )
    .unwrap();
}

fn knowledge_request() -> KnowledgePlanRequest {
    KnowledgePlanRequest {
        schema_version: CURRENT_SCHEMA_VERSION,
        changes: vec![KnowledgeChangeRequest {
            path: PortableRelativePath::parse("articles/composite.md").unwrap(),
            before_sha256: None,
            summary: "Save composite knowledge.".into(),
            content: "---\ntype: Article\ntitle: Composite save\nstatus: stable\ngenerated:\n  by: process:composite-save-test\n  at: 2026-09-08T00:00:00Z\nsources:\n  - id: source\n    resource: https://example.com/composite\nkb:\n  managed: true\n---\n\n# Composite save\n"
                .into(),
        }],
    }
}

fn context(base: &Path) -> AppContext {
    AppContext::new(
        BTreeMap::from([
            (
                "KB_CONFIG_DIR".to_owned(),
                base.join("config").display().to_string(),
            ),
            (
                "KB_STATE_DIR".to_owned(),
                base.join("state").display().to_string(),
            ),
            (
                "KB_CACHE_DIR".to_owned(),
                base.join("cache").display().to_string(),
            ),
        ]),
        base.to_path_buf(),
    )
}
