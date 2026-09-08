use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use kb_app::{
    AppContext, AppRequest, InitRequest, OperationRequest, OperationState, SkillRequest,
    SourceCapturePlan, SourceCaptureResult, attach_operation_summary, summary_for_knowledge_plan,
    summary_for_skill_plan, summary_for_skill_result, summary_for_source_plan,
    summary_for_source_result, summary_for_state,
};
use kb_core::{
    CURRENT_SCHEMA_VERSION, ErrorCode, KnowledgeChangeRequest, KnowledgePlan, KnowledgePlanRequest,
    KnowledgePlanResult, OperationId, OperationKind, PortableRelativePath, SkillAction,
    SkillApplyResult, SkillFileChange, SkillHost, SkillInstallMode, SkillPlan, SkillScope,
};
use serde_json::json;
use uuid::Uuid;

#[test]
fn planned_knowledge_summary_is_actionable_and_uses_vault_relative_paths() {
    let summary =
        summary_for_knowledge_plan(&plan_with_changes(["articles/one.md", "research/two.md"]));

    assert_eq!(summary.operation_state, "planned");
    assert!(summary.requires_confirmation);
    assert!(summary.can_apply);
    assert_eq!(summary.change_count, 2);
    assert_eq!(
        summary.affected_paths,
        vec!["Wiki/articles/one.md", "Wiki/research/two.md"]
    );
}

#[test]
fn applied_summary_keeps_the_full_id_but_cannot_be_applied_again() {
    let result = KnowledgePlanResult {
        kind: OperationKind::SaveKnowledge,
        operation_id: OperationId::new(),
        vault_id: Uuid::new_v4(),
        target: "/vault".into(),
        changed: vec![PortableRelativePath::parse("Wiki/articles/one.md").unwrap()],
        warnings: Vec::new(),
    };

    let summary = summary_for_state(&OperationState::AppliedKnowledge(result));

    assert_eq!(summary.operation_id.to_string().len(), 36);
    assert_eq!(summary.operation_state, "applied");
    assert!(!summary.requires_confirmation);
    assert!(!summary.can_apply);
}

#[test]
fn legacy_applied_source_summary_does_not_invent_paths_from_resource_identifiers() {
    let operation_id = OperationId::new();
    let vault_id = Uuid::new_v4();
    let digest = "a".repeat(64);
    let plan: SourceCapturePlan = serde_json::from_value(json!({
        "schema_version": CURRENT_SCHEMA_VERSION,
        "operation_id": operation_id,
        "kind": "capture_sources",
        "vault_id": vault_id,
        "target": "/vault",
        "admission_sha256": "b".repeat(64),
        "config_sha256": "c".repeat(64),
        "inputs": [{
            "version": {
                "source": {
                    "admission_id": "archive-id",
                    "relative_path": "folder name/file#.md"
                },
                "sha256": digest
            },
            "relative_path": "Research Library/folder name/file#.md",
            "media_type": "markdown",
            "present": true
        }],
        "writes": [],
        "created_at": "2026-09-08T00:00:00Z",
        "app_version": "test"
    }))
    .unwrap();
    let result: SourceCaptureResult = serde_json::from_value(json!({
        "kind": "capture_sources",
        "operation_id": operation_id,
        "vault_id": vault_id,
        "target": "/vault",
        "captured": [format!(
            "kb-source://archive-id/folder%20name/file%23.md?sha256={digest}"
        )],
        "marked_missing": [],
        "warnings": []
    }))
    .unwrap();

    let planned = summary_for_source_plan(&plan);
    let applied = summary_for_source_result(&result);

    assert_eq!(
        planned.affected_paths,
        vec!["Research Library/folder name/file#.md"]
    );
    assert!(applied.affected_paths.is_empty());
    assert_eq!(applied.change_count, 1);
}

#[test]
fn user_skill_summaries_ignore_misleading_root_components_and_use_portable_separators() {
    let operation_id = OperationId::new();
    let vault_id = Uuid::new_v4();
    let vault_root = PathBuf::from_iter(["vault-root"]);
    let changed = PathBuf::from_iter([
        "overrides",
        "skills",
        "tenant",
        ".codex",
        "skills",
        "kb-query",
        "SKILL.md",
    ]);
    let plan = SkillPlan {
        schema_version: CURRENT_SCHEMA_VERSION,
        operation_id,
        kind: OperationKind::ManageSkill,
        vault_id,
        vault_root: vault_root.clone(),
        host: SkillHost::Codex,
        scope: SkillScope::User,
        mode: SkillInstallMode::Copy,
        action: SkillAction::Install,
        files: vec![SkillFileChange {
            path: changed.clone(),
            before_sha256: None,
            after: Some("content".into()),
        }],
        links: Vec::new(),
        link: None,
        created_at: "2026-09-08T00:00:00Z".into(),
        app_version: "test".into(),
    };
    let result = SkillApplyResult {
        kind: OperationKind::ManageSkill,
        operation_id,
        vault_id,
        vault_root,
        host: SkillHost::Codex,
        scope: SkillScope::User,
        mode: SkillInstallMode::Copy,
        action: SkillAction::Install,
        changed: vec![changed],
        warnings: Vec::new(),
    };

    let planned = summary_for_skill_plan(&plan);
    let applied = summary_for_skill_result(&result);

    assert_eq!(
        planned.affected_paths,
        vec![".codex/skills/kb-query/SKILL.md"]
    );
    assert_eq!(
        applied.affected_paths,
        vec![".codex/skills/kb-query/SKILL.md"]
    );
    assert!(!planned.affected_paths[0].contains('\\'));
    assert!(!applied.affected_paths[0].contains('\\'));
}

#[test]
fn legacy_single_link_plan_deserializes_as_one_link() {
    let mut value = serde_json::to_value(SkillPlan {
        schema_version: CURRENT_SCHEMA_VERSION,
        operation_id: OperationId::new(),
        kind: OperationKind::ManageSkill,
        vault_id: Uuid::new_v4(),
        vault_root: "/vault".into(),
        host: SkillHost::Codex,
        scope: SkillScope::Vault,
        mode: SkillInstallMode::Symlink,
        action: SkillAction::Install,
        files: Vec::new(),
        links: Vec::new(),
        link: Some(kb_core::SkillLinkChange {
            path: "/vault/.agents/skills/knowledge-brain".into(),
            target: "/config/skills/knowledge-brain".into(),
            create: true,
        }),
        created_at: "2026-09-08T00:00:00Z".into(),
        app_version: "test".into(),
    })
    .unwrap();
    value.as_object_mut().unwrap().remove("links");
    let plan: SkillPlan = serde_json::from_value(value).unwrap();
    assert_eq!(plan.all_links().count(), 1);
}

#[test]
fn attaching_a_summary_preserves_existing_root_fields() {
    let plan = plan_with_changes(["articles/one.md"]);
    let summary = summary_for_knowledge_plan(&plan);

    let response = attach_operation_summary(
        json!({"operation_id": plan.operation_id, "changes": plan.changes}),
        summary,
    )
    .unwrap();

    assert_eq!(response["operation_id"], plan.operation_id.to_string());
    assert_eq!(response["changes"].as_array().unwrap().len(), 1);
    assert_eq!(
        response["operation_summary"]["operation_id"],
        plan.operation_id.to_string()
    );
}

#[test]
fn every_planning_route_adds_one_root_level_operation_summary() {
    let temporary = tempfile::tempdir().unwrap();
    let context = context(temporary.path());

    let adopt_target = temporary.path().join("adopt-me");
    fs::create_dir(&adopt_target).unwrap();
    let adoption = kb_app::run(
        AppRequest::Adopt {
            target: adopt_target,
        },
        &context,
    )
    .unwrap();
    assert_planned_summary(&adoption, "adopt_vault");

    let vault = temporary.path().join("vault");
    kb_app::run(
        AppRequest::Init(InitRequest {
            target: vault.clone(),
        }),
        &context,
    )
    .unwrap();

    fs::create_dir(vault.join("Notes")).unwrap();
    fs::write(vault.join("Notes/one.md"), "# One\n").unwrap();
    fs::write(
        vault.join("admission.yml"),
        "schema_version: v1.0\ndirectories:\n  - id: notes\n    path: Notes\n    enabled: true\n",
    )
    .unwrap();
    let review = kb_app::run(
        AppRequest::Review {
            vault: Some(vault.display().to_string()),
        },
        &context,
    )
    .unwrap();
    assert!(review["operation_id"].is_string());
    assert_planned_summary(&review, "capture_sources");
    assert_eq!(
        review["operation_summary"]["affected_paths"],
        json!(["Notes/one.md"])
    );

    let knowledge = kb_app::run(
        AppRequest::PlanCreate {
            vault: Some(vault.display().to_string()),
            request: knowledge_request("articles/one.md"),
        },
        &context,
    )
    .unwrap();
    assert!(knowledge["changes"].is_array());
    assert_planned_summary(&knowledge, "save_knowledge");

    let skill = kb_app::run(
        AppRequest::Skills(SkillRequest::Install {
            vault: Some(vault.display().to_string()),
            host: Some(SkillHost::Codex),
            scope: SkillScope::Vault,
            mode: SkillInstallMode::Copy,
        }),
        &context,
    )
    .unwrap();
    assert!(skill["files"].is_array());
    assert_planned_summary(&skill, "manage_skill");
    assert!(
        skill["operation_summary"]["affected_paths"]
            .as_array()
            .unwrap()
            .iter()
            .all(|path| !path.as_str().unwrap().starts_with('/'))
    );
}

#[test]
fn failed_operation_show_uses_the_terminal_event_without_hiding_the_plan() {
    let temporary = tempfile::tempdir().unwrap();
    let context = context(temporary.path());
    let target = temporary.path().join("stale-adoption");
    fs::create_dir(&target).unwrap();
    fs::write(target.join("note.md"), "Before\n").unwrap();
    let plan = kb_app::run(
        AppRequest::Adopt {
            target: target.clone(),
        },
        &context,
    )
    .unwrap();
    let operation_id = plan["operation_id"].as_str().unwrap().parse().unwrap();
    fs::write(target.join("note.md"), "After\n").unwrap();

    let error = kb_app::run(AppRequest::Apply { operation_id }, &context).unwrap_err();
    assert_eq!(error.code, ErrorCode::PlanStale);
    let shown = kb_app::run(
        AppRequest::Operation(OperationRequest::Show { operation_id }),
        &context,
    )
    .unwrap();

    assert_eq!(shown["state"], "planned");
    assert!(shown["plan"].is_object());
    assert_eq!(
        shown["operation_summary"]["operation_id"],
        operation_id.to_string()
    );
    assert_eq!(shown["operation_summary"]["operation_state"], "failed");
    assert_eq!(shown["operation_summary"]["requires_confirmation"], false);
    assert_eq!(shown["operation_summary"]["can_apply"], false);
}

#[test]
fn legacy_operation_show_without_events_remains_planned_and_applicable() {
    let temporary = tempfile::tempdir().unwrap();
    let context = context(temporary.path());
    let target = temporary.path().join("legacy-adoption");
    fs::create_dir(&target).unwrap();
    let plan = kb_app::run(AppRequest::Adopt { target }, &context).unwrap();
    let operation_id: OperationId = plan["operation_id"].as_str().unwrap().parse().unwrap();
    let events = temporary
        .path()
        .join("state/operations")
        .join(operation_id.to_string())
        .join("events.json");
    fs::remove_file(&events).unwrap();

    let shown = kb_app::run(
        AppRequest::Operation(OperationRequest::Show { operation_id }),
        &context,
    )
    .unwrap();

    assert_eq!(shown["state"], "planned");
    assert_eq!(shown["operation_summary"]["operation_state"], "planned");
    assert_eq!(shown["operation_summary"]["requires_confirmation"], true);
    assert_eq!(shown["operation_summary"]["can_apply"], true);
    assert!(!events.exists());
}

#[test]
fn real_source_apply_and_show_preserve_source_relative_affected_paths() {
    let temporary = tempfile::tempdir().unwrap();
    let context = context(temporary.path());
    let vault = temporary.path().join("vault");
    kb_app::run(
        AppRequest::Init(InitRequest {
            target: vault.clone(),
        }),
        &context,
    )
    .unwrap();
    fs::create_dir_all(vault.join("Research Library/folder name")).unwrap();
    fs::write(
        vault.join("Research Library/folder name/file#.md"),
        "# Evidence\n",
    )
    .unwrap();
    fs::write(
        vault.join("admission.yml"),
        "schema_version: v1.0\ndirectories:\n  - id: archive-id\n    path: Research Library\n    enabled: true\n",
    )
    .unwrap();

    let plan = kb_app::run(
        AppRequest::Review {
            vault: Some(vault.display().to_string()),
        },
        &context,
    )
    .unwrap();
    let operation_id: OperationId = plan["operation_id"].as_str().unwrap().parse().unwrap();
    assert_eq!(
        plan["operation_summary"]["affected_paths"],
        json!(["Research Library/folder name/file#.md"])
    );

    kb_app::run(AppRequest::Apply { operation_id }, &context).unwrap();
    let shown = kb_app::run(
        AppRequest::Operation(OperationRequest::Show { operation_id }),
        &context,
    )
    .unwrap();

    assert_eq!(shown["state"], "applied");
    assert_eq!(shown["result"]["operation_id"], operation_id.to_string());
    assert_eq!(
        shown["result"]["source_paths"],
        json!(["Research Library/folder name/file#.md"])
    );
    assert_eq!(
        shown["operation_summary"]["affected_paths"],
        json!(["Research Library/folder name/file#.md"])
    );
    assert_eq!(
        shown["operation_summary"]["operation_id"],
        operation_id.to_string()
    );
}

#[test]
fn skill_uninstall_apply_and_show_keep_additive_roots_and_full_identity() {
    let temporary = tempfile::tempdir().unwrap();
    let context = context(temporary.path());
    let vault = temporary.path().join("vault");
    kb_app::run(
        AppRequest::Init(InitRequest {
            target: vault.clone(),
        }),
        &context,
    )
    .unwrap();
    let install = kb_app::run(
        AppRequest::Skills(SkillRequest::Install {
            vault: Some(vault.display().to_string()),
            host: Some(SkillHost::Codex),
            scope: SkillScope::Vault,
            mode: SkillInstallMode::Copy,
        }),
        &context,
    )
    .unwrap();
    let install_id = install["operation_id"].as_str().unwrap().parse().unwrap();
    kb_app::run(
        AppRequest::Apply {
            operation_id: install_id,
        },
        &context,
    )
    .unwrap();

    let uninstall = kb_app::run(
        AppRequest::Skills(SkillRequest::Uninstall {
            vault: Some(vault.display().to_string()),
            host: Some(SkillHost::Codex),
            scope: SkillScope::Vault,
        }),
        &context,
    )
    .unwrap();
    let operation_id: OperationId = uninstall["operation_id"].as_str().unwrap().parse().unwrap();
    assert_eq!(operation_id.to_string().len(), 36);
    assert_eq!(uninstall["action"], "uninstall");
    assert_eq!(
        uninstall["operation_summary"]["operation_id"],
        operation_id.to_string()
    );

    kb_app::run(AppRequest::Apply { operation_id }, &context).unwrap();
    let shown = kb_app::run(
        AppRequest::Operation(OperationRequest::Show { operation_id }),
        &context,
    )
    .unwrap();

    assert_eq!(shown["state"], "applied");
    assert_eq!(shown["result"]["action"], "uninstall");
    assert_eq!(shown["result"]["operation_id"], operation_id.to_string());
    assert_eq!(
        shown["operation_summary"]["operation_id"],
        operation_id.to_string()
    );
    assert_eq!(shown["operation_summary"]["operation_state"], "applied");
    assert_eq!(shown["operation_summary"]["can_apply"], false);
    assert!(
        shown["operation_summary"]["affected_paths"]
            .as_array()
            .unwrap()
            .iter()
            .all(|path| !path.as_str().unwrap().starts_with('/'))
    );
}

#[test]
fn operation_show_preserves_the_plan_and_marks_applied_results_unavailable() {
    let temporary = tempfile::tempdir().unwrap();
    let context = context(temporary.path());
    let vault = temporary.path().join("vault");
    kb_app::run(
        AppRequest::Init(InitRequest {
            target: vault.clone(),
        }),
        &context,
    )
    .unwrap();
    let plan = kb_app::run(
        AppRequest::PlanCreate {
            vault: Some(vault.display().to_string()),
            request: knowledge_request("articles/show.md"),
        },
        &context,
    )
    .unwrap();
    let operation_id = plan["operation_id"].as_str().unwrap().parse().unwrap();

    let shown = kb_app::run(
        AppRequest::Operation(OperationRequest::Show { operation_id }),
        &context,
    )
    .unwrap();
    assert_eq!(shown["state"], "planned");
    assert_eq!(shown["plan"]["operation_id"], operation_id.to_string());
    assert_eq!(shown["operation_summary"]["requires_confirmation"], true);

    kb_app::run(AppRequest::Apply { operation_id }, &context).unwrap();
    let applied = kb_app::run(
        AppRequest::Operation(OperationRequest::Show { operation_id }),
        &context,
    )
    .unwrap();
    assert_eq!(applied["state"], "applied");
    assert_eq!(applied["result"]["operation_id"], operation_id.to_string());
    assert_eq!(applied["operation_summary"]["can_apply"], false);
    assert_eq!(applied["operation_summary"]["requires_confirmation"], false);
}

fn plan_with_changes<const N: usize>(paths: [&str; N]) -> KnowledgePlan {
    KnowledgePlan {
        schema_version: CURRENT_SCHEMA_VERSION,
        operation_id: OperationId::new(),
        kind: OperationKind::SaveKnowledge,
        vault_id: Uuid::new_v4(),
        target: "/vault".into(),
        changes: paths
            .into_iter()
            .map(|path| KnowledgeChangeRequest {
                path: PortableRelativePath::parse(path).unwrap(),
                before_sha256: None,
                summary: "Save knowledge.".into(),
                content: "content".into(),
            })
            .collect(),
        source_versions: Vec::new(),
        admission_sha256: "a".repeat(64),
        config_sha256: "b".repeat(64),
        writes: Vec::new(),
        diff: String::new(),
        created_at: "2026-09-08T00:00:00Z".into(),
        app_version: "test".into(),
    }
}

fn knowledge_request(path: &str) -> KnowledgePlanRequest {
    KnowledgePlanRequest {
        schema_version: CURRENT_SCHEMA_VERSION,
        changes: vec![KnowledgeChangeRequest {
            path: PortableRelativePath::parse(path).unwrap(),
            before_sha256: None,
            summary: "Save knowledge.".into(),
            content: "---\ntype: Article\ntitle: One\nstatus: stable\ngenerated:\n  by: process:test\n  at: 2026-09-08T00:00:00Z\nsources:\n  - id: source\n    resource: https://example.com/source\nkb:\n  managed: true\n---\n\n# One\n"
                .into(),
        }],
    }
}

fn context(base: &Path) -> AppContext {
    let environment = BTreeMap::from([
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
        (
            "KB_AGENT_HOME".to_owned(),
            base.join("home").display().to_string(),
        ),
        (
            "KB_AGENT_CONFIG_DIR".to_owned(),
            base.join("agent-config").display().to_string(),
        ),
    ]);
    AppContext::new(environment, base.to_path_buf())
}

fn assert_planned_summary(response: &serde_json::Value, operation_kind: &str) {
    assert!(response.get("operation_summary").is_some());
    assert_eq!(
        response["operation_summary"]["operation_id"],
        response["operation_id"]
    );
    assert_eq!(
        response["operation_summary"]["operation_kind"],
        operation_kind
    );
    assert_eq!(response["operation_summary"]["operation_state"], "planned");
    assert_eq!(response["operation_summary"]["requires_confirmation"], true);
    assert_eq!(response["operation_summary"]["can_apply"], true);
}
