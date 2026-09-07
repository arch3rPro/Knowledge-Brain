use std::{collections::BTreeMap, fs, path::Path};

use kb_app::{
    AppContext, AppRequest, InitRequest, OperationRequest, OperationState, SkillRequest,
    attach_operation_summary, summary_for_knowledge_plan, summary_for_state,
};
use kb_core::{
    CURRENT_SCHEMA_VERSION, KnowledgeChangeRequest, KnowledgePlan, KnowledgePlanRequest,
    KnowledgePlanResult, OperationId, OperationKind, PortableRelativePath, SkillHost,
    SkillInstallMode, SkillScope,
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
