use std::path::{Component, Path, PathBuf};

use kb_core::{
    AdoptionPlan, KbError, KnowledgePlan, KnowledgePlanResult, OperationId, SkillAction,
    SkillApplyResult, SkillPlan,
};
use serde::Serialize;
use uuid::Uuid;

use crate::{AdoptionResult, OperationState, SourceCapturePlan, SourceCaptureResult};

#[derive(Debug, Clone, Serialize)]
pub struct OperationSummary {
    pub operation_id: OperationId,
    pub operation_kind: String,
    pub operation_state: String,
    pub vault_id: Uuid,
    pub vault_root: PathBuf,
    pub change_count: usize,
    pub affected_paths: Vec<String>,
    pub summary: String,
    pub requires_confirmation: bool,
    pub can_apply: bool,
}

#[must_use]
pub fn summary_for_adoption_plan(plan: &AdoptionPlan) -> OperationSummary {
    planned_summary(
        plan.operation_id,
        "adopt_vault",
        plan.vault_id,
        &plan.target,
        plan.creates
            .iter()
            .map(|file| file.relative_path.as_str().to_owned())
            .collect(),
        "Adopt Vault",
    )
}

#[must_use]
pub fn summary_for_adoption_result(result: &AdoptionResult) -> OperationSummary {
    applied_summary(
        result.operation_id,
        "adopt_vault",
        result.vault_id,
        &result.target,
        result.created.clone(),
        "Adopt Vault",
    )
}

#[must_use]
pub fn summary_for_source_plan(plan: &SourceCapturePlan) -> OperationSummary {
    planned_summary(
        plan.operation_id,
        "capture_sources",
        plan.vault_id,
        &plan.target,
        plan.inputs
            .iter()
            .map(|input| input.relative_path.as_str().to_owned())
            .collect(),
        "Capture sources",
    )
}

#[must_use]
pub fn summary_for_source_result(result: &SourceCaptureResult) -> OperationSummary {
    let affected_paths = result
        .captured
        .iter()
        .chain(&result.marked_missing)
        .map(|source| source_relative_path(source))
        .collect();
    applied_summary(
        result.operation_id,
        "capture_sources",
        result.vault_id,
        &result.target,
        affected_paths,
        "Capture sources",
    )
}

#[must_use]
pub fn summary_for_knowledge_plan(plan: &KnowledgePlan) -> OperationSummary {
    planned_summary(
        plan.operation_id,
        "save_knowledge",
        plan.vault_id,
        &plan.target,
        plan.changes
            .iter()
            .map(|change| format!("Wiki/{}", change.path.as_str()))
            .collect(),
        "Save knowledge",
    )
}

#[must_use]
pub fn summary_for_knowledge_result(result: &KnowledgePlanResult) -> OperationSummary {
    applied_summary(
        result.operation_id,
        "save_knowledge",
        result.vault_id,
        &result.target,
        result
            .changed
            .iter()
            .map(|path| path.as_str().to_owned())
            .collect(),
        "Save knowledge",
    )
}

#[must_use]
pub fn summary_for_skill_plan(plan: &SkillPlan) -> OperationSummary {
    let affected_paths = plan
        .files
        .iter()
        .map(|change| host_relative_path(&change.path, &plan.vault_root))
        .chain(
            plan.link
                .iter()
                .map(|change| host_relative_path(&change.path, &plan.vault_root)),
        )
        .collect();
    planned_summary(
        plan.operation_id,
        "manage_skill",
        plan.vault_id,
        &plan.vault_root,
        affected_paths,
        skill_action_label(plan.action),
    )
}

#[must_use]
pub fn summary_for_skill_result(result: &SkillApplyResult) -> OperationSummary {
    applied_summary(
        result.operation_id,
        "manage_skill",
        result.vault_id,
        &result.vault_root,
        result
            .changed
            .iter()
            .map(|path| host_relative_path(path, &result.vault_root))
            .collect(),
        skill_action_label(result.action),
    )
}

#[must_use]
pub fn summary_for_state(state: &OperationState) -> OperationSummary {
    match state {
        OperationState::Planned(plan) => summary_for_adoption_plan(plan),
        OperationState::Applied(result) => summary_for_adoption_result(result),
        OperationState::PlannedSource(plan) => summary_for_source_plan(plan),
        OperationState::AppliedSource(result) => summary_for_source_result(result),
        OperationState::PlannedKnowledge(plan) => summary_for_knowledge_plan(plan),
        OperationState::AppliedKnowledge(result) => summary_for_knowledge_result(result),
        OperationState::PlannedSkill(plan) => summary_for_skill_plan(plan),
        OperationState::AppliedSkill(result) => summary_for_skill_result(result),
    }
}

/// Append a typed operation summary without changing any existing response field.
///
/// # Errors
///
/// Returns [`KbError`] when the response is not a JSON object or serialization fails.
pub fn attach_operation_summary(
    mut response: serde_json::Value,
    summary: OperationSummary,
) -> Result<serde_json::Value, KbError> {
    response
        .as_object_mut()
        .ok_or_else(|| KbError::invalid_config("operation response", "expected object"))?
        .insert(
            "operation_summary".to_owned(),
            serde_json::to_value(summary)
                .map_err(|error| KbError::invalid_config("operation summary", error.to_string()))?,
        );
    Ok(response)
}

fn planned_summary(
    operation_id: OperationId,
    operation_kind: &str,
    vault_id: Uuid,
    vault_root: &Path,
    affected_paths: Vec<String>,
    action: &str,
) -> OperationSummary {
    let change_count = affected_paths.len();
    OperationSummary {
        operation_id,
        operation_kind: operation_kind.to_owned(),
        operation_state: "planned".to_owned(),
        vault_id,
        vault_root: vault_root.to_path_buf(),
        change_count,
        affected_paths,
        summary: format!("{action}: {change_count} planned change(s)."),
        requires_confirmation: true,
        can_apply: true,
    }
}

fn applied_summary(
    operation_id: OperationId,
    operation_kind: &str,
    vault_id: Uuid,
    vault_root: &Path,
    affected_paths: Vec<String>,
    action: &str,
) -> OperationSummary {
    let change_count = affected_paths.len();
    OperationSummary {
        operation_id,
        operation_kind: operation_kind.to_owned(),
        operation_state: "applied".to_owned(),
        vault_id,
        vault_root: vault_root.to_path_buf(),
        change_count,
        affected_paths,
        summary: format!("{action}: {change_count} applied change(s)."),
        requires_confirmation: false,
        can_apply: false,
    }
}

fn skill_action_label(action: SkillAction) -> &'static str {
    match action {
        SkillAction::Install => "Install Skills",
        SkillAction::Uninstall => "Uninstall Skills",
    }
}

fn source_relative_path(source: &str) -> String {
    source
        .strip_prefix("kb-source://")
        .unwrap_or(source)
        .split('?')
        .next()
        .unwrap_or(source)
        .to_owned()
}

fn host_relative_path(path: &Path, vault_root: &Path) -> String {
    if let Ok(relative) = path.strip_prefix(vault_root) {
        return portable_path(relative);
    }
    let components = path.components().collect::<Vec<_>>();
    let start = components.iter().position(|component| {
        matches!(
            component,
            Component::Normal(value)
                if matches!(value.to_str(), Some(".agents" | ".codex" | ".claude" | ".gemini" | ".opencode" | "opencode" | "skills"))
        )
    });
    start.map_or_else(
        || portable_path(path),
        |index| {
            components[index..]
                .iter()
                .collect::<PathBuf>()
                .display()
                .to_string()
        },
    )
}

fn portable_path(path: &Path) -> String {
    path.components()
        .filter_map(|component| match component {
            Component::Normal(value) => value.to_str(),
            Component::ParentDir => Some(".."),
            Component::CurDir => Some("."),
            Component::RootDir | Component::Prefix(_) => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}
