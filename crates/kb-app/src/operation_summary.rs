use std::path::{Component, Path, PathBuf};

use kb_core::{
    AdoptionPlan, KbError, KnowledgePlan, KnowledgePlanResult, OperationEventKind, OperationId,
    SkillAction, SkillApplyResult, SkillHost, SkillPlan, SkillScope,
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
    let change_count = result.captured.len() + result.marked_missing.len();
    let affected_paths = result
        .source_paths
        .iter()
        .map(|path| path.as_str().to_owned())
        .collect();
    let mut summary = applied_summary(
        result.operation_id,
        "capture_sources",
        result.vault_id,
        &result.target,
        affected_paths,
        "Capture sources",
    );
    summary.change_count = change_count;
    summary.summary = format!("Capture sources: {change_count} applied change(s).");
    summary
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
        .map(|change| host_relative_path(&change.path, &plan.vault_root, plan.scope, plan.host))
        .chain(plan.all_links().map(|change| {
            host_relative_path(&change.path, &plan.vault_root, plan.scope, plan.host)
        }))
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
            .map(|path| host_relative_path(path, &result.vault_root, result.scope, result.host))
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

pub(crate) fn summary_for_state_with_event(
    state: &OperationState,
    latest_event: OperationEventKind,
) -> OperationSummary {
    let mut summary = summary_for_state(state);
    if matches!(
        state,
        OperationState::Applied(_)
            | OperationState::AppliedSource(_)
            | OperationState::AppliedKnowledge(_)
            | OperationState::AppliedSkill(_)
    ) {
        return summary;
    }
    let (operation_state, state_phrase, requires_confirmation, can_apply) = match latest_event {
        OperationEventKind::Planned => ("planned", "planned", true, true),
        OperationEventKind::Applying => ("applying", "being applied", false, true),
        OperationEventKind::Progress => ("progress", "in progress", false, true),
        OperationEventKind::Recovering => ("recovering", "recovering", false, true),
        OperationEventKind::Applied => ("applied", "applied", false, false),
        OperationEventKind::Failed => ("failed", "failed", false, false),
    };
    summary.operation_state = operation_state.to_owned();
    summary.summary = format!(
        "{}: {} {state_phrase} change(s).",
        state_action_label(state),
        summary.change_count
    );
    summary.requires_confirmation = requires_confirmation;
    summary.can_apply = can_apply;
    summary
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

fn state_action_label(state: &OperationState) -> &'static str {
    match state {
        OperationState::Planned(_) | OperationState::Applied(_) => "Adopt Vault",
        OperationState::PlannedSource(_) | OperationState::AppliedSource(_) => "Capture sources",
        OperationState::PlannedKnowledge(_) | OperationState::AppliedKnowledge(_) => {
            "Save knowledge"
        }
        OperationState::PlannedSkill(plan) => skill_action_label(plan.action),
        OperationState::AppliedSkill(result) => skill_action_label(result.action),
    }
}

fn host_relative_path(
    path: &Path,
    vault_root: &Path,
    scope: SkillScope,
    host: SkillHost,
) -> String {
    if scope == SkillScope::Vault {
        if let Ok(relative) = path.strip_prefix(vault_root) {
            return portable_path(relative);
        }
    }
    let components = path.components().collect::<Vec<_>>();
    let host_root = match host {
        SkillHost::Codex => ".codex",
        SkillHost::ClaudeCode => ".claude",
        SkillHost::GeminiCli => ".gemini",
        SkillHost::OpenCode => "opencode",
    };
    let start = components
        .iter()
        .rposition(|component| component.as_os_str() == host_root)
        .or_else(|| {
            components
                .iter()
                .rposition(|component| component.as_os_str() == "skills")
        });
    start.map_or_else(
        || portable_path(path),
        |index| portable_components(&components[index..]),
    )
}

fn portable_path(path: &Path) -> String {
    portable_components(&path.components().collect::<Vec<_>>())
}

fn portable_components(components: &[Component<'_>]) -> String {
    components
        .iter()
        .filter_map(|component| match component {
            Component::Normal(value) => value.to_str(),
            Component::ParentDir => Some(".."),
            Component::CurDir => Some("."),
            Component::RootDir | Component::Prefix(_) => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}
