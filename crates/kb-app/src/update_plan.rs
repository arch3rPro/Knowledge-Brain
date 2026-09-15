use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use kb_core::{
    ErrorCode, KbError, ManagedSkillInstallation, OperationId, SchemaCompatibility, SkillAction,
    SkillScope, UpdateComponent, UpdateComponentKind, UpdateComponentState, UpdateConflict,
    UpdateFileAction, UpdateFileChange, UpdateOperation, UpdateOwnership, UpdatePhase, UpdatePlan,
    UpdatePlanState, UpdateScope, UpdateScopeMode, find_vault_root,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use time::{Duration, OffsetDateTime, format_description::well_known::Rfc3339};
use uuid::Uuid;

use crate::{
    AgentRoots, ConfigOverrides, IndexCompatibility, ResolvedVault, SkillInstallState,
    SkillPlanRequest, UpdateStore, UserPaths, VaultSelection, inspect_index,
    list_managed_skill_installations, list_vaults, load_effective_config, plan_template_update,
    preview_skill_plan, resolve_vault, skill_status,
};

#[derive(Debug, Clone)]
pub struct UpdateRuntime {
    pub identity: kb_update::BuildIdentity,
    pub executable: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum UpdatePlanningOutcome {
    Plan {
        plan: UpdatePlan,
        #[serde(skip_serializing_if = "Option::is_none")]
        confirmation_token: Option<kb_core::UpdateConfirmationToken>,
    },
    Existing {
        operation: UpdateOperation,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredUpdateStage {
    pub version: String,
    pub target: String,
    pub executable_relative: PathBuf,
    pub executable_sha256: String,
    pub archive_sha256: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateSelection {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vault: Option<String>,
    #[serde(default)]
    pub excluded_vaults: Vec<Uuid>,
    #[serde(default)]
    pub persist: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkippedUpdateVault {
    pub vault_id: Uuid,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedUpdateScope {
    pub mode: UpdateScopeMode,
    pub vaults: Vec<ResolvedVault>,
    pub excluded_vaults: Vec<Uuid>,
    pub skipped_vaults: Vec<SkippedUpdateVault>,
    pub managed_skills: Vec<ManagedSkillInstallation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TargetPlanRequest {
    pub operation_id: OperationId,
    pub current_version: String,
    pub target_version: String,
    pub executable_path: PathBuf,
    pub executable_before_sha256: Option<String>,
    pub executable_after_sha256: Option<String>,
    pub executable_managed: bool,
    pub scope: ResolvedUpdateScope,
    pub user_paths: UserPaths,
    pub agent_roots: AgentRoots,
    #[serde(default)]
    pub environment: BTreeMap<String, String>,
    pub created_at: String,
    pub expires_at: String,
}

impl TargetPlanRequest {
    /// Create a target-planning request with a one-hour confirmation lifetime.
    ///
    /// # Errors
    ///
    /// Returns an error only when an RFC 3339 timestamp cannot be formatted.
    pub fn new(
        current_version: impl Into<String>,
        target_version: impl Into<String>,
        executable_path: PathBuf,
        scope: ResolvedUpdateScope,
        user_paths: UserPaths,
        agent_roots: AgentRoots,
    ) -> Result<Self, KbError> {
        let now = OffsetDateTime::now_utc();
        Ok(Self {
            operation_id: OperationId::new(),
            current_version: current_version.into(),
            target_version: target_version.into(),
            executable_path,
            executable_before_sha256: None,
            executable_after_sha256: None,
            executable_managed: false,
            scope,
            user_paths,
            agent_roots,
            environment: BTreeMap::new(),
            created_at: format_time(now)?,
            expires_at: format_time(now + Duration::hours(1))?,
        })
    }
}

/// Resolve update targets from explicit, current-Vault, or registered scope.
///
/// Unresolved registrations are reported rather than blocking other Vaults.
/// Only durable Knowledge-Brain Skill ownership records are returned; external
/// and missing optional Skills are never adopted.
///
/// # Errors
///
/// Returns an error for an invalid explicit selection, registry, or managed
/// Skill ownership store.
pub fn resolve_update_scope(
    user_paths: &UserPaths,
    current_dir: &Path,
    environment: &BTreeMap<String, String>,
    selection: &UpdateSelection,
) -> Result<ResolvedUpdateScope, KbError> {
    let vault_selection = |explicit| VaultSelection {
        explicit,
        environment: environment.clone(),
        current_dir: current_dir.to_path_buf(),
    };
    let (mode, mut vaults, mut skipped_vaults) = if selection.vault.is_some()
        || environment.contains_key("KB_VAULT")
    {
        (
            UpdateScopeMode::SelectedVaults,
            vec![resolve_vault(
                user_paths,
                &vault_selection(selection.vault.clone()),
            )?],
            Vec::new(),
        )
    } else if let Ok(root) = find_vault_root(current_dir) {
        (
            UpdateScopeMode::CurrentVault,
            vec![resolve_vault(
                user_paths,
                &vault_selection(Some(root.to_string_lossy().into_owned())),
            )?],
            Vec::new(),
        )
    } else {
        let (vaults, skipped) = resolve_registered_vaults(user_paths, current_dir, environment)?;
        (UpdateScopeMode::RegisteredVaults, vaults, skipped)
    };

    vaults.sort_by_key(|vault| vault.vault_id);
    let selected_ids = vaults
        .iter()
        .map(|vault| vault.vault_id)
        .collect::<BTreeSet<_>>();
    let excluded = selection
        .excluded_vaults
        .iter()
        .copied()
        .filter(|vault_id| selected_ids.contains(vault_id))
        .collect::<BTreeSet<_>>();
    vaults.retain(|vault| !excluded.contains(&vault.vault_id));

    let remaining_ids = vaults
        .iter()
        .map(|vault| vault.vault_id)
        .collect::<BTreeSet<_>>();
    let managed_skills = list_managed_skill_installations(user_paths)?
        .into_iter()
        .filter(|record| {
            record.scope == SkillScope::User || remaining_ids.contains(&record.vault_id)
        })
        .collect();
    skipped_vaults.sort_by_key(|vault| vault.vault_id);

    Ok(ResolvedUpdateScope {
        mode,
        vaults,
        excluded_vaults: excluded.into_iter().collect(),
        skipped_vaults,
        managed_skills,
    })
}

/// Build a complete target-version plan using this executable's embedded
/// templates and Skills without changing managed state.
///
/// # Errors
///
/// Returns an error when the target identity, selected paths, or readable
/// managed state is invalid.
pub fn create_target_update_plan(request: &TargetPlanRequest) -> Result<UpdatePlan, KbError> {
    if request
        .environment
        .keys()
        .any(|key| !TARGET_ENVIRONMENT_KEYS.contains(&key.as_str()))
    {
        return Err(KbError::invalid_config(
            "target update request",
            "environment contains an unsupported key",
        ));
    }
    if request.target_version != env!("CARGO_PKG_VERSION") {
        return Err(KbError::new(
            ErrorCode::UpdateVerificationFailed,
            "The target planner version does not match the requested release.",
            false,
            "Discard the staged executable and prepare the update again.",
        ));
    }
    let mut components = vec![executable_component(request)];
    let mut conflicts = Vec::new();
    let mut skipped = request
        .scope
        .skipped_vaults
        .iter()
        .map(|vault| format!("vault:{}: {}", vault.vault_id, vault.reason))
        .collect::<Vec<_>>();

    for vault in &request.scope.vaults {
        plan_vault_components(
            request,
            vault,
            &mut components,
            &mut conflicts,
            &mut skipped,
        )?;
    }
    for installation in &request.scope.managed_skills {
        components.push(skill_component(request, installation, &mut conflicts)?);
    }
    components.sort_by(|left, right| left.id.cmp(&right.id));
    conflicts.sort_by(|left, right| {
        (&left.component_id, &left.path, &left.reason).cmp(&(
            &right.component_id,
            &right.path,
            &right.reason,
        ))
    });
    skipped.sort();
    skipped.dedup();

    let has_changes = components.iter().any(|component| {
        matches!(
            component.state,
            UpdateComponentState::Pending | UpdateComponentState::RebuildRequired
        )
    });
    let plan = UpdatePlan {
        kind: "update".into(),
        phase: UpdatePhase::Preview,
        state: if has_changes {
            UpdatePlanState::ReviewRequired
        } else {
            UpdatePlanState::NoChanges
        },
        operation_id: request.operation_id,
        current_version: request.current_version.clone(),
        target_version: request.target_version.clone(),
        scope: UpdateScope {
            mode: request.scope.mode,
            vaults: request
                .scope
                .vaults
                .iter()
                .map(|vault| vault.vault_id)
                .collect(),
        },
        excluded_vaults: request.scope.excluded_vaults.clone(),
        components,
        conflicts,
        skipped,
        untouched: vec![
            "admission choices".into(),
            "external Skills".into(),
            "Git metadata".into(),
            "missing optional Skills".into(),
            "Obsidian settings".into(),
            "ordinary notes".into(),
            "topic directories".into(),
            "Wiki content".into(),
        ],
        created_at: request.created_at.clone(),
        expires_at: request.expires_at.clone(),
    };
    plan.validate()?;
    Ok(plan)
}

/// Resolve, verify, and optionally persist one complete update preview.
///
/// # Errors
///
/// Returns an error for invalid scope, release verification, target planning,
/// or durable storage.
pub fn plan_update(
    runtime: &UpdateRuntime,
    user_paths: &UserPaths,
    agent_roots: &AgentRoots,
    current_dir: &Path,
    environment: &BTreeMap<String, String>,
    selection: &UpdateSelection,
    persist: bool,
) -> Result<UpdatePlanningOutcome, KbError> {
    let store = UpdateStore::new(user_paths);
    if let Some(operation) = store.latest()? {
        if !operation.execution_state.is_terminal() {
            return Ok(UpdatePlanningOutcome::Existing { operation });
        }
    }
    let scope = resolve_update_scope(user_paths, current_dir, environment, selection)?;
    let current_digest = fs::read(&runtime.executable)
        .map(|bytes| digest(&bytes))
        .map_err(|error| io_error("read current executable", &runtime.executable, &error))?;
    let mut request = TargetPlanRequest::new(
        runtime.identity.version().to_string(),
        runtime.identity.version().to_string(),
        runtime.executable.clone(),
        scope,
        user_paths.clone(),
        agent_roots.clone(),
    )?;
    request.executable_before_sha256 = Some(current_digest.clone());
    request.executable_after_sha256 = Some(current_digest);
    request.executable_managed = runtime.identity.can_update();
    request.environment = target_environment(environment);

    let mut attached_stage = None;
    let plan = if runtime.identity.can_update() {
        let check = kb_update::check_for_update(&runtime.identity, &kb_update::UreqTransport)
            .map_err(|error| release_error(&error))?;
        if let Some(release) = check.latest {
            request.target_version = release.version.to_string();
            let stage = if persist {
                store.create_stage()?
            } else {
                tempfile::tempdir().map_err(|error| {
                    io_error("create temporary update stage", &runtime.executable, &error)
                })?
            };
            let verified = kb_update::verify_release(
                &runtime.identity,
                release,
                &kb_update::UreqTransport,
                stage.path(),
            )
            .map_err(|error| release_error(&error))?;
            validate_staged_executable(&verified)?;
            request.executable_after_sha256 = Some(verified.executable_sha256.clone());
            let target_plan = invoke_target_planner(&verified.executable, &request)?;
            if persist {
                attached_stage = Some((
                    stage,
                    StoredUpdateStage {
                        version: verified.version.to_string(),
                        target: verified.target.triple().into(),
                        executable_relative: PathBuf::from("stage")
                            .join("verified")
                            .join(verified.target.executable_name()),
                        executable_sha256: verified.executable_sha256,
                        archive_sha256: verified.sha256,
                    },
                ));
            }
            target_plan
        } else {
            create_target_update_plan(&request)?
        }
    } else {
        create_target_update_plan(&request)?
    };

    if !persist || plan.state == UpdatePlanState::NoChanges {
        return Ok(UpdatePlanningOutcome::Plan {
            plan,
            confirmation_token: None,
        });
    }
    let token = plan.confirmation_token()?;
    store.create(&plan)?;
    if let Some((stage, metadata)) = attached_stage {
        store.attach_stage(plan.operation_id, stage, &metadata)?;
    }
    Ok(UpdatePlanningOutcome::Plan {
        plan,
        confirmation_token: Some(token),
    })
}

/// Ask a verified target executable to interpret its own embedded update assets.
///
/// # Errors
///
/// Returns an error when the process fails, emits invalid JSON, or returns a
/// plan for a different operation, target version, or Vault scope.
pub fn invoke_target_planner(
    executable: &Path,
    request: &TargetPlanRequest,
) -> Result<UpdatePlan, KbError> {
    let temporary = tempfile::tempdir()
        .map_err(|error| io_error("create target planner directory", executable, &error))?;
    let request_path = temporary.path().join("request.json");
    let output_path = temporary.path().join("plan.json");
    let bytes = serde_json::to_vec_pretty(request)
        .map_err(|error| KbError::invalid_config("target update request", error.to_string()))?;
    crate::atomic_replace(&request_path, &bytes)?;
    let output = scrubbed_command(executable)
        .args(["__update-plan", "--request"])
        .arg(&request_path)
        .arg("--output")
        .arg(&output_path)
        .output()
        .map_err(|error| io_error("run target update planner", executable, &error))?;
    if !output.status.success() {
        return Err(KbError::new(
            ErrorCode::UpdateVerificationFailed,
            "The verified target executable could not create its update plan.",
            false,
            "Discard the staged release and prepare the update again.",
        ));
    }
    let plan_bytes = fs::read(&output_path)
        .map_err(|error| io_error("read target update plan", &output_path, &error))?;
    let plan: UpdatePlan = serde_json::from_slice(&plan_bytes).map_err(|error| {
        KbError::invalid_config(output_path.display().to_string(), error.to_string())
    })?;
    plan.validate()?;
    let expected_vaults = request
        .scope
        .vaults
        .iter()
        .map(|vault| vault.vault_id)
        .collect::<Vec<_>>();
    if plan.operation_id != request.operation_id
        || plan.target_version != request.target_version
        || plan.current_version != request.current_version
        || plan.scope.mode != request.scope.mode
        || plan.scope.vaults != expected_vaults
        || plan.excluded_vaults != request.scope.excluded_vaults
        || plan.phase != UpdatePhase::Preview
    {
        return Err(KbError::new(
            ErrorCode::UpdateVerificationFailed,
            "The target executable returned a plan for a different update request.",
            false,
            "Discard the staged release and prepare the update again.",
        ));
    }
    Ok(plan)
}

fn resolve_registered_vaults(
    user_paths: &UserPaths,
    current_dir: &Path,
    environment: &BTreeMap<String, String>,
) -> Result<(Vec<ResolvedVault>, Vec<SkippedUpdateVault>), KbError> {
    let mut vaults = Vec::new();
    let mut skipped = Vec::new();
    for record in list_vaults(user_paths)? {
        let selection = VaultSelection {
            explicit: Some(record.vault_id.to_string()),
            environment: environment.clone(),
            current_dir: current_dir.to_path_buf(),
        };
        match resolve_vault(user_paths, &selection) {
            Ok(vault) => vaults.push(vault),
            Err(error) => skipped.push(SkippedUpdateVault {
                vault_id: record.vault_id,
                reason: error.to_string(),
            }),
        }
    }
    Ok((vaults, skipped))
}

fn executable_component(request: &TargetPlanRequest) -> UpdateComponent {
    let (state, changes, message) = if !request.executable_managed {
        (
            UpdateComponentState::Skipped,
            Vec::new(),
            "The executable is owned by a package manager, Cargo, or a source build.".into(),
        )
    } else if request.executable_before_sha256 == request.executable_after_sha256 {
        (
            UpdateComponentState::Unchanged,
            Vec::new(),
            "The official executable is current.".into(),
        )
    } else {
        (
            UpdateComponentState::Pending,
            vec![UpdateFileChange {
                path: request.executable_path.clone(),
                ownership: UpdateOwnership::Executable,
                action: UpdateFileAction::Replace,
                before_sha256: request.executable_before_sha256.clone(),
                after_sha256: request.executable_after_sha256.clone(),
                diff: None,
            }],
            "Replace the official executable with the verified target release.".into(),
        )
    };
    UpdateComponent {
        id: "executable".into(),
        kind: UpdateComponentKind::Executable,
        state,
        vault_id: None,
        host: None,
        skill_scope: None,
        from_version: Some(request.current_version.clone()),
        to_version: Some(request.target_version.clone()),
        changes,
        message,
    }
}

fn plan_vault_components(
    request: &TargetPlanRequest,
    vault: &ResolvedVault,
    components: &mut Vec<UpdateComponent>,
    conflicts: &mut Vec<UpdateConflict>,
    skipped: &mut Vec<String>,
) -> Result<(), KbError> {
    let identity = crate::vault::read_vault_identity(&vault.root)?;
    let compatibility = crate::schema::vault_schema_compatibility(identity.schema_version);
    if compatibility != SchemaCompatibility::Current {
        let id = format!("vault:{}:data-schema", vault.vault_id);
        components.push(UpdateComponent {
            id: id.clone(),
            kind: UpdateComponentKind::DataSchema,
            state: UpdateComponentState::Skipped,
            vault_id: Some(vault.vault_id),
            host: None,
            skill_scope: None,
            from_version: Some(identity.schema_version.to_string()),
            to_version: Some(kb_core::CURRENT_SCHEMA_VERSION.to_string()),
            changes: Vec::new(),
            message: format!(
                "Data schema compatibility is {compatibility:?}; no complete update migration is available in this workflow."
            ),
        });
        skipped.push(id);
        return Ok(());
    }

    let template = plan_template_update(&vault.root)?;
    let template_id = format!("vault:{}:template", vault.vault_id);
    conflicts.extend(template.conflicts.iter().cloned().map(|mut conflict| {
        conflict.component_id.clone_from(&template_id);
        conflict
    }));
    components.push(UpdateComponent {
        id: template_id,
        kind: UpdateComponentKind::VaultTemplate,
        state: template.component_state,
        vault_id: Some(vault.vault_id),
        host: None,
        skill_scope: None,
        from_version: template
            .from_template_version
            .map(|version| version.to_string()),
        to_version: Some(template.to_template_version.to_string()),
        changes: template.changes,
        message: if template.writes.is_empty() {
            "No safe Vault template write is required.".into()
        } else {
            "Update the complete product-managed Vault template boundary.".into()
        },
    });

    let config = load_effective_config(
        &vault.root,
        &request.user_paths,
        &ConfigOverrides {
            environment: request.environment.clone(),
            cli: BTreeMap::new(),
        },
    )?;
    let index_state = inspect_index(&vault.root, &config)?;
    let rebuild = index_state != IndexCompatibility::Current;
    let index_path = vault.root.join(".kb/cache/catalog.json");
    components.push(UpdateComponent {
        id: format!("vault:{}:index", vault.vault_id),
        kind: UpdateComponentKind::SearchIndex,
        state: if rebuild {
            UpdateComponentState::RebuildRequired
        } else {
            UpdateComponentState::Unchanged
        },
        vault_id: Some(vault.vault_id),
        host: None,
        skill_scope: None,
        from_version: None,
        to_version: None,
        changes: rebuild
            .then(|| UpdateFileChange {
                before_sha256: file_digest(&index_path),
                path: index_path,
                ownership: UpdateOwnership::RebuildableIndex,
                action: UpdateFileAction::Rebuild,
                after_sha256: None,
                diff: None,
            })
            .into_iter()
            .collect(),
        message: format!("Local search index compatibility is {index_state:?}."),
    });
    Ok(())
}

pub(crate) fn skill_component(
    request: &TargetPlanRequest,
    installation: &ManagedSkillInstallation,
    conflicts: &mut Vec<UpdateConflict>,
) -> Result<UpdateComponent, KbError> {
    let id = format!(
        "skill:{}:{}",
        match installation.scope {
            SkillScope::Vault => "vault",
            SkillScope::User => "user",
        },
        installation.host.as_str()
    );
    let vault_root = request
        .scope
        .vaults
        .iter()
        .find(|vault| vault.vault_id == installation.vault_id)
        .map_or(installation.skills_root.as_path(), |vault| {
            vault.root.as_path()
        });
    let status = skill_status(
        &request.user_paths,
        vault_root,
        installation.vault_id,
        &request.agent_roots,
        installation.host,
        installation.scope,
    )?;
    let mut changes = Vec::new();
    let state = match status.state {
        SkillInstallState::Current => UpdateComponentState::Unchanged,
        SkillInstallState::Outdated => {
            let skill_plan = preview_skill_plan(&SkillPlanRequest {
                vault_root,
                vault_id: installation.vault_id,
                user_paths: &request.user_paths,
                roots: &request.agent_roots,
                host: installation.host,
                scope: installation.scope,
                mode: installation.mode,
                action: SkillAction::Install,
            })?;
            changes.extend(skill_plan.files.into_iter().map(|change| {
                let ownership = if installation.bridge_file.as_ref() == Some(&change.path) {
                    UpdateOwnership::ManagedBridge
                } else {
                    UpdateOwnership::ManagedSkill
                };
                UpdateFileChange {
                    action: match (&change.before_sha256, &change.after) {
                        (_, None) => UpdateFileAction::Delete,
                        (None, Some(_)) => UpdateFileAction::Create,
                        (Some(_), Some(_)) => UpdateFileAction::Update,
                    },
                    path: change.path,
                    ownership,
                    before_sha256: change.before_sha256,
                    after_sha256: change.after.as_deref().map(text_digest),
                    diff: None,
                }
            }));
            changes.extend(skill_plan.links.into_iter().map(|link| UpdateFileChange {
                path: link.path,
                ownership: UpdateOwnership::Symlink,
                action: if link.create {
                    UpdateFileAction::Create
                } else {
                    UpdateFileAction::Delete
                },
                before_sha256: None,
                after_sha256: None,
                diff: Some(format!("target: {}", link.target.display())),
            }));
            UpdateComponentState::Pending
        }
        state => {
            conflicts.push(UpdateConflict {
                component_id: id.clone(),
                path: Some(status.skills_root),
                reason: format!("Managed Skill installation is {state:?}."),
                next_action: "Restore the recorded managed baseline or update this installation through its owning installer.".into(),
            });
            UpdateComponentState::Skipped
        }
    };
    Ok(UpdateComponent {
        id,
        kind: UpdateComponentKind::ManagedSkill,
        state,
        vault_id: (installation.scope == SkillScope::Vault).then_some(installation.vault_id),
        host: Some(installation.host),
        skill_scope: Some(installation.scope),
        from_version: None,
        to_version: Some(request.target_version.clone()),
        changes,
        message: format!("Managed Skill installation is {:?}.", status.state),
    })
}

fn file_digest(path: &Path) -> Option<String> {
    fs::read(path).ok().map(|bytes| digest(&bytes))
}

fn text_digest(text: &str) -> String {
    digest(text.as_bytes())
}

fn digest(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn format_time(value: OffsetDateTime) -> Result<String, KbError> {
    value
        .format(&Rfc3339)
        .map_err(|error| KbError::invalid_config("update timestamp", error.to_string()))
}

const TARGET_ENVIRONMENT_KEYS: &[&str] = &[
    "KB_SEARCH_MODE",
    "KB_LIMITS_MAX_FILE_BYTES",
    "KB_LIMITS_MAX_FILES_PER_REVIEW",
    "KB_LIMITS_MAX_TOTAL_READ_BYTES",
    "KB_FILES_INCLUDE_HIDDEN",
    "KB_OPERATIONS_PLAN_RETENTION_HOURS",
];

pub(crate) fn target_environment(
    environment: &BTreeMap<String, String>,
) -> BTreeMap<String, String> {
    environment
        .iter()
        .filter(|(key, _)| TARGET_ENVIRONMENT_KEYS.contains(&key.as_str()))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect()
}

fn scrubbed_command(program: &Path) -> Command {
    let environment = std::env::vars_os().filter(|(name, _)| {
        let name = name.to_string_lossy().to_ascii_uppercase();
        !["KEY", "SECRET", "TOKEN", "PASSWORD"]
            .iter()
            .any(|sensitive| name.contains(sensitive))
    });
    let mut command = Command::new(program);
    command.env_clear().envs(environment);
    command
}

fn validate_staged_executable(verified: &kb_update::VerifiedArchive) -> Result<(), KbError> {
    let output = Command::new(&verified.executable)
        .args(["version", "--json"])
        .output()
        .map_err(|error| io_error("run staged executable", &verified.executable, &error))?;
    if !output.status.success() {
        return Err(KbError::new(
            ErrorCode::UpdateVerificationFailed,
            "The staged executable could not report its identity.",
            false,
            "Discard the staged release and prepare the update again.",
        ));
    }
    kb_update::validate_staged_identity(&output.stdout, &verified.version, verified.target)
        .map_err(|error| release_error(&error))
}

fn release_error(error: &kb_update::UpdateError) -> KbError {
    let retryable = error
        .transport_failure()
        .is_some_and(kb_update::TransportFailure::retryable);
    let code = if matches!(error, kb_update::UpdateError::VerificationFailed(_)) {
        ErrorCode::UpdateVerificationFailed
    } else {
        ErrorCode::IoFailure
    };
    KbError::new(
        code,
        "Cannot prepare the official update release.",
        retryable,
        "Review the reported release failure and retry only when it is safe.",
    )
    .with_details(serde_json::json!({ "reason": error.to_string() }))
}

fn io_error(action: &str, path: &Path, error: &std::io::Error) -> KbError {
    KbError::io_failure(action, path.display().to_string(), error.to_string())
}
