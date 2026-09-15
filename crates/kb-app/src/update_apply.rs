use std::collections::BTreeMap;
use std::fs;

use kb_core::{
    ErrorCode, KbError, OperationId, SkillAction, SkillScope, UpdateComponent, UpdateComponentKind,
    UpdateComponentState, UpdateExecutionState, UpdatePlan,
};

use crate::{
    AgentRoots, ConfigOverrides, ResolvedUpdateScope, ResolvedVault, SkillPlanRequest,
    TargetPlanRequest, UpdateStore, UserPaths, VaultLock, VaultSelection, apply_skill_plan_direct,
    apply_template_update, list_managed_skill_installations, load_effective_config,
    plan_template_update, preview_skill_plan, rebuild_catalog, resolve_vault,
    update_plan::{skill_component, target_environment},
};

pub(crate) fn resume_update_components(
    user_paths: &UserPaths,
    roots: &AgentRoots,
    environment: &BTreeMap<String, String>,
    operation_id: OperationId,
) -> Result<kb_core::UpdateOperation, KbError> {
    let store = UpdateStore::new(user_paths);
    let plan = store.load_plan(operation_id)?;
    let mut operation = store.load(operation_id)?;
    operation = match operation.execution_state {
        UpdateExecutionState::Confirmed | UpdateExecutionState::CliReplaced => {
            store.transition(operation_id, UpdateExecutionState::ApplyingComponents)?
        }
        UpdateExecutionState::ApplyingComponents => operation,
        state if state.is_terminal() => return Ok(operation),
        _ => {
            return Err(KbError::new(
                ErrorCode::InvalidConfig,
                "The update is not ready to apply managed components.",
                false,
                "Inspect update status before resuming.",
            ));
        }
    };

    let vaults = resolve_operation_vaults(user_paths, environment, &plan)?;
    let installations = list_managed_skill_installations(user_paths)?;
    let target_request = target_request_for_resume(
        user_paths,
        roots,
        environment,
        &plan,
        vaults.clone(),
        installations.clone(),
    )?;

    if let Some(component) = operation
        .components
        .iter()
        .find(|component| {
            component.kind == UpdateComponentKind::Executable
                && component.state == UpdateComponentState::Pending
        })
        .cloned()
    {
        let mut receipt = component;
        receipt.state = if verify_executable_result(&receipt) {
            UpdateComponentState::Applied
        } else {
            receipt.message =
                "Installed executable digest differs from the confirmed target.".into();
            UpdateComponentState::Failed
        };
        operation = store.record_component(operation_id, receipt)?;
    }

    for component in operation.components.clone() {
        if !matches!(
            component.state,
            UpdateComponentState::Pending | UpdateComponentState::RebuildRequired
        ) || component.kind == UpdateComponentKind::Executable
        {
            continue;
        }
        let result = apply_component(
            user_paths,
            roots,
            environment,
            &component,
            &vaults,
            &installations,
            &target_request,
            operation_id,
        );
        let mut receipt = component;
        receipt.state = match result {
            Ok(()) => UpdateComponentState::Applied,
            Err(error) if receipt.kind == UpdateComponentKind::SearchIndex => {
                receipt.message = error.to_string();
                UpdateComponentState::RebuildRequired
            }
            Err(error) if error.code == ErrorCode::PlanStale => {
                receipt.message = error.to_string();
                UpdateComponentState::Stale
            }
            Err(error) => {
                receipt.message = error.to_string();
                UpdateComponentState::Failed
            }
        };
        operation = store.record_component(operation_id, receipt)?;
    }

    let terminal = aggregate_state(&operation.components);
    store.transition(operation_id, terminal)
}

#[allow(clippy::too_many_arguments)]
fn apply_component(
    user_paths: &UserPaths,
    roots: &AgentRoots,
    environment: &BTreeMap<String, String>,
    component: &UpdateComponent,
    vaults: &[ResolvedVault],
    installations: &[kb_core::ManagedSkillInstallation],
    target_request: &TargetPlanRequest,
    operation_id: OperationId,
) -> Result<(), KbError> {
    match component.kind {
        UpdateComponentKind::VaultTemplate => {
            let vault = component_vault(component, vaults)?;
            crate::app::ensure_mutation_allowed(&vault.root)?;
            let _lock = VaultLock::acquire(
                &vault.root,
                crate::LockMode::Exclusive,
                "apply update template",
                Some(operation_id),
            )?;
            let fresh = plan_template_update(&vault.root)?;
            if fresh.changes != component.changes || !fresh.conflicts.is_empty() {
                return Err(stale(component));
            }
            apply_template_update(&vault.root, &fresh)?;
            Ok(())
        }
        UpdateComponentKind::SearchIndex => {
            let vault = component_vault(component, vaults)?;
            verify_change_preconditions(component)?;
            crate::app::ensure_mutation_allowed(&vault.root)?;
            let _lock = VaultLock::acquire(
                &vault.root,
                crate::LockMode::Exclusive,
                "rebuild update index",
                Some(operation_id),
            )?;
            let config = load_effective_config(
                &vault.root,
                user_paths,
                &ConfigOverrides {
                    environment: environment.clone(),
                    cli: BTreeMap::new(),
                },
            )?;
            rebuild_catalog(&vault.root, &config)?;
            Ok(())
        }
        UpdateComponentKind::ManagedSkill => {
            let installation = installations
                .iter()
                .find(|record| {
                    Some(record.host) == component.host
                        && Some(record.scope) == component.skill_scope
                        && (record.scope == SkillScope::User
                            || Some(record.vault_id) == component.vault_id)
                })
                .ok_or_else(|| stale(component))?;
            let mut fresh_conflicts = Vec::new();
            let fresh = skill_component(target_request, installation, &mut fresh_conflicts)?;
            if fresh.changes != component.changes
                || fresh.state != UpdateComponentState::Pending
                || !fresh_conflicts.is_empty()
            {
                return Err(stale(component));
            }
            let vault_root = vaults
                .iter()
                .find(|vault| vault.vault_id == installation.vault_id)
                .map_or(installation.skills_root.as_path(), |vault| {
                    vault.root.as_path()
                });
            let mut skill_plan = preview_skill_plan(&SkillPlanRequest {
                vault_root,
                vault_id: installation.vault_id,
                user_paths,
                roots,
                host: installation.host,
                scope: installation.scope,
                mode: installation.mode,
                action: SkillAction::Install,
            })?;
            skill_plan.operation_id = operation_id;
            apply_skill_plan_direct(user_paths, roots, &skill_plan)?;
            Ok(())
        }
        UpdateComponentKind::DataSchema => Err(KbError::new(
            ErrorCode::MigrationUnavailable,
            "No registered data-schema migration is available.",
            false,
            "Upgrade with a version that contains the required migration.",
        )),
        UpdateComponentKind::Executable => Ok(()),
    }
}

fn verify_change_preconditions(component: &UpdateComponent) -> Result<(), KbError> {
    for change in &component.changes {
        let actual = match fs::read(&change.path) {
            Ok(bytes) => {
                use sha2::{Digest, Sha256};
                Some(hex::encode(Sha256::digest(bytes)))
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => {
                return Err(KbError::io_failure(
                    "verify update precondition",
                    change.path.display().to_string(),
                    error.to_string(),
                ));
            }
        };
        if actual != change.before_sha256 {
            return Err(stale(component));
        }
    }
    Ok(())
}

fn verify_executable_result(component: &UpdateComponent) -> bool {
    component.changes.len() == 1
        && component.changes[0]
            .after_sha256
            .as_ref()
            .is_some_and(|expected| {
                fs::read(&component.changes[0].path)
                    .ok()
                    .map(|bytes| {
                        use sha2::{Digest, Sha256};
                        hex::encode(Sha256::digest(bytes))
                    })
                    .as_ref()
                    == Some(expected)
            })
}

fn resolve_operation_vaults(
    user_paths: &UserPaths,
    environment: &BTreeMap<String, String>,
    plan: &UpdatePlan,
) -> Result<Vec<ResolvedVault>, KbError> {
    plan.scope
        .vaults
        .iter()
        .map(|vault_id| {
            resolve_vault(
                user_paths,
                &VaultSelection {
                    explicit: Some(vault_id.to_string()),
                    environment: environment.clone(),
                    // The explicit stable ID selects the Vault. A deterministic local
                    // directory avoids making recovery depend on the helper's cwd.
                    current_dir: user_paths.state_dir.clone(),
                },
            )
        })
        .collect()
}

fn target_request_for_resume(
    user_paths: &UserPaths,
    roots: &AgentRoots,
    environment: &BTreeMap<String, String>,
    plan: &UpdatePlan,
    vaults: Vec<ResolvedVault>,
    managed_skills: Vec<kb_core::ManagedSkillInstallation>,
) -> Result<TargetPlanRequest, KbError> {
    let executable_path = std::env::current_exe().map_err(|error| {
        KbError::io_failure("resolve current executable", ".", error.to_string())
    })?;
    Ok(TargetPlanRequest {
        operation_id: plan.operation_id,
        current_version: plan.current_version.clone(),
        target_version: plan.target_version.clone(),
        executable_path,
        executable_before_sha256: None,
        executable_after_sha256: None,
        executable_managed: false,
        scope: ResolvedUpdateScope {
            mode: plan.scope.mode,
            vaults,
            excluded_vaults: plan.excluded_vaults.clone(),
            skipped_vaults: Vec::new(),
            managed_skills,
        },
        user_paths: user_paths.clone(),
        agent_roots: roots.clone(),
        environment: target_environment(environment),
        created_at: plan.created_at.clone(),
        expires_at: plan.expires_at.clone(),
    })
}

fn component_vault<'a>(
    component: &UpdateComponent,
    vaults: &'a [ResolvedVault],
) -> Result<&'a ResolvedVault, KbError> {
    let vault_id = component
        .vault_id
        .ok_or_else(|| KbError::invalid_config("update component", "missing Vault identity"))?;
    vaults
        .iter()
        .find(|vault| vault.vault_id == vault_id)
        .ok_or_else(|| stale(component))
}

fn aggregate_state(components: &[UpdateComponent]) -> UpdateExecutionState {
    let has_problem = components.iter().any(|component| {
        matches!(
            component.state,
            UpdateComponentState::Stale
                | UpdateComponentState::Failed
                | UpdateComponentState::RebuildRequired
                | UpdateComponentState::Pending
        )
    });
    let has_success = components.iter().any(|component| {
        matches!(
            component.state,
            UpdateComponentState::Applied | UpdateComponentState::Unchanged
        )
    });
    if has_problem {
        if has_success {
            UpdateExecutionState::Partial
        } else {
            UpdateExecutionState::Failed
        }
    } else if components
        .iter()
        .any(|component| component.state == UpdateComponentState::Skipped)
    {
        UpdateExecutionState::CompletedWithSkips
    } else {
        UpdateExecutionState::Completed
    }
}

fn stale(component: &UpdateComponent) -> KbError {
    KbError::new(
        ErrorCode::PlanStale,
        format!(
            "Update component {} changed after confirmation.",
            component.id
        ),
        false,
        "Keep the current file and create a new update plan.",
    )
}
