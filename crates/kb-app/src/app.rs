use std::{collections::BTreeMap, path::PathBuf};

use kb_core::{
    CURRENT_SCHEMA_VERSION, ErrorCode, KbError, OperationId, SchemaCompatibility, SkillAction,
    SkillHost, SkillInstallMode, SkillScope,
};
use serde::Serialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::{
    AdmissionAction, ConfigOverrides, ConfigTarget, InitRequest, LockMode, OperationState,
    UserPaths, VaultLock, VaultSelection, admission_change, apply_operation, capabilities,
    config_get, config_set, config_show, config_unset, config_validate, create_adoption_plan,
    doctor, init_and_register_vault, init_vault, inspect_operation, list_vaults, load_admission,
    rebind_vault, register_vault, resolve_skill_host, resolve_vault, skill_target,
    summary_for_adoption_plan, summary_for_knowledge_plan, summary_for_skill_plan,
    summary_for_state, unregister_vault, vault_status,
};

#[derive(Debug, Clone)]
pub struct AppContext {
    environment: BTreeMap<String, String>,
    current_dir: PathBuf,
    user_paths: Result<UserPaths, KbError>,
}

pub type AppResponse = Value;

impl AppContext {
    #[must_use]
    pub fn new(environment: BTreeMap<String, String>, current_dir: PathBuf) -> Self {
        let user_paths = UserPaths::resolve(&environment);
        Self {
            environment,
            current_dir,
            user_paths,
        }
    }

    fn user_paths(&self) -> Result<&UserPaths, KbError> {
        self.user_paths.as_ref().map_err(Clone::clone)
    }

    fn overrides(&self) -> ConfigOverrides {
        ConfigOverrides {
            environment: self.environment.clone(),
            cli: BTreeMap::new(),
        }
    }

    fn agent_roots(&self) -> Result<crate::AgentRoots, KbError> {
        crate::AgentRoots::resolve(&self.environment)
    }
}

#[derive(Debug, Clone)]
pub enum AppRequest {
    Backup(BackupRequest),
    Review {
        vault: Option<String>,
    },
    Maintenance {
        vault: Option<String>,
    },
    SourceSave {
        vault: Option<String>,
        mode: SaveMode,
    },
    Query {
        vault: Option<String>,
        request: kb_core::SearchRequest,
    },
    Lint {
        vault: Option<String>,
    },
    PlanCreate {
        vault: Option<String>,
        request: kb_core::KnowledgePlanRequest,
    },
    KnowledgeSave {
        vault: Option<String>,
        request: Option<kb_core::KnowledgePlanRequest>,
        mode: SaveMode,
    },
    CacheRebuild {
        vault: Option<String>,
    },
    SourceVerify {
        vault: Option<String>,
    },
    Skills(SkillRequest),
    Init(InitRequest),
    Adopt {
        target: PathBuf,
    },
    Apply {
        operation_id: OperationId,
    },
    ApplyForVault {
        vault: String,
        operation_id: OperationId,
    },
    Operation(OperationRequest),
    Config(ConfigRequest),
    Status {
        vault: Option<String>,
    },
    Doctor {
        vault: Option<String>,
    },
    Vault(VaultRequest),
    Paths {
        vault: Option<String>,
    },
    Version,
    Capabilities,
}

#[derive(Debug, Clone)]
pub enum SaveMode {
    Prepare,
    Confirm(OperationId),
    ApplyImmediately,
}

#[derive(Debug, Clone)]
pub enum SkillRequest {
    Detect {
        vault: Option<String>,
    },
    Install {
        vault: Option<String>,
        host: Option<SkillHost>,
        scope: SkillScope,
        mode: SkillInstallMode,
    },
    Status {
        vault: Option<String>,
        host: Option<SkillHost>,
        scope: SkillScope,
    },
    Uninstall {
        vault: Option<String>,
        host: Option<SkillHost>,
        scope: SkillScope,
    },
}

#[derive(Debug, Clone)]
pub enum BackupRequest {
    Create {
        vault: Option<String>,
        output: Option<PathBuf>,
        without_source_objects: bool,
    },
    Verify {
        archive: PathBuf,
    },
    Restore {
        archive: PathBuf,
        target: PathBuf,
    },
}

#[derive(Debug, Clone)]
pub enum OperationRequest {
    Show {
        operation_id: OperationId,
    },
    ShowForVault {
        vault: String,
        operation_id: OperationId,
    },
    EventsForVault {
        vault: String,
        operation_id: OperationId,
    },
}

#[derive(Debug, Clone)]
pub enum ConfigRequest {
    Show {
        vault: Option<String>,
        sources: bool,
    },
    Get {
        vault: Option<String>,
        key: String,
    },
    Set {
        vault: Option<String>,
        target: ConfigTarget,
        key: String,
        value: String,
        write: bool,
    },
    Unset {
        vault: Option<String>,
        target: ConfigTarget,
        key: String,
        write: bool,
    },
    Validate {
        vault: Option<String>,
    },
    Admission {
        vault: Option<String>,
        request: AdmissionRequest,
    },
}

#[derive(Debug, Clone)]
pub enum AdmissionRequest {
    List,
    Change {
        action: AdmissionAction,
        write: bool,
    },
}

#[derive(Debug, Clone)]
pub enum VaultRequest {
    List,
    Register { path: PathBuf },
    Rebind { vault_id: Uuid, path: PathBuf },
    Unregister { vault_id: Uuid },
}

/// Execute one application request independently of CLI, GUI, or web adapters.
///
/// # Errors
///
/// Returns a stable [`KbError`] for invalid input, unavailable capabilities, or
/// failed storage operations.
pub fn run(request: AppRequest, context: &AppContext) -> Result<AppResponse, KbError> {
    match request {
        AppRequest::Backup(request) => run_backup(request, context),
        AppRequest::Review { vault } => run_review(context, vault),
        AppRequest::Maintenance { vault } => run_maintenance(context, vault),
        AppRequest::SourceSave { vault, mode } => run_source_save(context, vault, mode),
        AppRequest::Query { vault, request } => {
            request.validate()?;
            let selected = select_vault(context, vault)?;
            let _lock = VaultLock::acquire(&selected.root, LockMode::Shared, "query", None)?;
            crate::source_apply::ensure_no_pending(&selected.root)?;
            let config = crate::load_effective_config(
                &selected.root,
                context.user_paths()?,
                &context.overrides(),
            )?;
            to_value(crate::query(&selected.root, &request, &config)?)
        }
        AppRequest::Lint { vault } => run_lint(context, vault),
        AppRequest::PlanCreate { vault, request } => run_plan_create(context, vault, request),
        AppRequest::KnowledgeSave {
            vault,
            request,
            mode,
        } => run_knowledge_save(context, vault, request, mode),
        AppRequest::CacheRebuild { vault } => {
            let selected = select_vault(context, vault)?;
            ensure_mutation_allowed(&selected.root)?;
            let _lock =
                VaultLock::acquire(&selected.root, LockMode::Exclusive, "cache rebuild", None)?;
            crate::source_apply::ensure_no_pending(&selected.root)?;
            let config = crate::load_effective_config(
                &selected.root,
                context.user_paths()?,
                &context.overrides(),
            )?;
            to_value(crate::rebuild_catalog(&selected.root, &config)?)
        }
        AppRequest::SourceVerify { vault } => {
            let selected = select_vault(context, vault)?;
            let _lock =
                VaultLock::acquire(&selected.root, LockMode::Shared, "source verify", None)?;
            crate::source_apply::ensure_no_pending(&selected.root)?;
            let config = crate::load_effective_config(
                &selected.root,
                context.user_paths()?,
                &context.overrides(),
            )?;
            to_value(crate::verify_sources(&selected.root, &config)?)
        }
        AppRequest::Skills(request) => run_skills(&request, context),
        AppRequest::Init(request) => run_init(&request, context),
        AppRequest::Adopt { target } => run_adopt(context, &target),
        AppRequest::Apply { operation_id } => run_apply(context, operation_id),
        AppRequest::ApplyForVault {
            vault,
            operation_id,
        } => run_apply_for_vault(context, &vault, operation_id),
        AppRequest::Operation(request) => run_operation(request, context),
        AppRequest::Config(request) => run_config(request, context),
        AppRequest::Status { vault } => {
            let selected = select_vault(context, vault)?;
            let _lock = VaultLock::acquire(&selected.root, LockMode::Shared, "status", None)?;
            to_value(vault_status(
                &selected.root,
                context.user_paths()?,
                &context.overrides(),
            )?)
        }
        AppRequest::Doctor { vault } => {
            let root = select_doctor_root(context, vault)?;
            to_value(doctor(&root, context.user_paths()?, &context.overrides())?)
        }
        AppRequest::Vault(request) => run_vault(request, context),
        AppRequest::Paths { vault } => run_paths(context, vault),
        AppRequest::Version => Ok(json!({
            "app_version": env!("CARGO_PKG_VERSION"),
            "schema_version": CURRENT_SCHEMA_VERSION,
        })),
        AppRequest::Capabilities => to_value(capabilities()),
    }
}

fn run_maintenance(context: &AppContext, vault: Option<String>) -> Result<Value, KbError> {
    let selected = select_vault(context, vault)?;
    let status = {
        let _lock = VaultLock::acquire(&selected.root, LockMode::Shared, "maintenance", None)?;
        vault_status(&selected.root, context.user_paths()?, &context.overrides())?
    };
    let (sources, lint) = if status.recovery.pending_operations > 0 {
        let unavailable = json!({
            "status": "not_checked",
            "reason": "vault_needs_recovery",
        });
        (unavailable.clone(), unavailable)
    } else {
        let config = crate::load_effective_config(
            &selected.root,
            context.user_paths()?,
            &context.overrides(),
        )?;
        let _lock = VaultLock::acquire(&selected.root, LockMode::Shared, "maintenance", None)?;
        (
            to_value(crate::inspect_source_changes(&selected.root, &config)?)?,
            to_value(crate::lint(
                &selected.root,
                &config,
                time::OffsetDateTime::now_utc(),
            )?)?,
        )
    };
    let diagnostics = doctor(&selected.root, context.user_paths()?, &context.overrides())?;
    Ok(json!({
        "kind": "maintenance",
        "schema_version": CURRENT_SCHEMA_VERSION,
        "root": selected.root,
        "status": status,
        "sources": sources,
        "lint": lint,
        "doctor": diagnostics,
    }))
}

fn run_backup(request: BackupRequest, context: &AppContext) -> Result<Value, KbError> {
    match request {
        BackupRequest::Create {
            vault,
            output,
            without_source_objects,
        } => {
            let selected = select_vault(context, vault)?;
            let _lock =
                VaultLock::acquire(&selected.root, LockMode::Shared, "backup create", None)?;
            crate::source_apply::ensure_no_pending(&selected.root)?;
            let created_at = time::OffsetDateTime::now_utc();
            let output = output.map_or_else(
                || {
                    let name = selected
                        .root
                        .file_name()
                        .and_then(std::ffi::OsStr::to_str)
                        .unwrap_or("knowledge-brain");
                    selected
                        .root
                        .parent()
                        .unwrap_or(&selected.root)
                        .join(format!("{name}-{}.kb.zip", created_at.unix_timestamp()))
                },
                |path| resolve_context_path(context, &path),
            );
            to_value(crate::create_backup(&crate::BackupCreateRequest {
                vault: selected.root,
                output,
                include_source_objects: !without_source_objects,
                created_at,
            })?)
        }
        BackupRequest::Verify { archive } => to_value(crate::verify_backup(
            &resolve_context_path(context, &archive),
        )?),
        BackupRequest::Restore { archive, target } => to_value(crate::restore_backup(
            &resolve_context_path(context, &archive),
            &resolve_context_path(context, &target),
        )?),
    }
}

fn resolve_context_path(context: &AppContext, path: &std::path::Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        context.current_dir.join(path)
    }
}

fn run_lint(context: &AppContext, vault: Option<String>) -> Result<Value, KbError> {
    let selected = select_vault(context, vault)?;
    let _lock = VaultLock::acquire(&selected.root, LockMode::Shared, "lint", None)?;
    crate::source_apply::ensure_no_pending(&selected.root)?;
    let config =
        crate::load_effective_config(&selected.root, context.user_paths()?, &context.overrides())?;
    to_value(crate::lint(
        &selected.root,
        &config,
        time::OffsetDateTime::now_utc(),
    )?)
}

fn run_review(context: &AppContext, vault: Option<String>) -> Result<Value, KbError> {
    let selected = select_vault(context, vault)?;
    ensure_mutation_allowed(&selected.root)?;
    let _lock = VaultLock::acquire(&selected.root, LockMode::Shared, "review", None)?;
    crate::source_apply::ensure_no_pending(&selected.root)?;
    let config =
        crate::load_effective_config(&selected.root, context.user_paths()?, &context.overrides())?;
    let report = crate::review_sources(&selected.root, context.user_paths()?, &config)?;
    let response = to_value(&report)?;
    if let Some(operation_id) = report.operation_id {
        let state = inspect_operation(context.user_paths()?, operation_id)?;
        crate::attach_operation_summary(response, summary_for_state(&state))
    } else {
        Ok(response)
    }
}

fn run_source_save(
    context: &AppContext,
    vault: Option<String>,
    mode: SaveMode,
) -> Result<Value, KbError> {
    match mode {
        SaveMode::Confirm(token) => confirm_save(context, vault, token, SaveKind::Source),
        mode @ (SaveMode::Prepare | SaveMode::ApplyImmediately) => {
            let preview = run_review(context, vault.clone())?;
            let operation_id = operation_id_from_preview(&preview)?;
            respond_to_prepared_save(
                context,
                vault,
                operation_id,
                &preview,
                &mode,
                SaveKind::Source,
            )
        }
    }
}

fn run_knowledge_save(
    context: &AppContext,
    vault: Option<String>,
    request: Option<kb_core::KnowledgePlanRequest>,
    mode: SaveMode,
) -> Result<Value, KbError> {
    match mode {
        SaveMode::Confirm(token) => confirm_save(context, vault, token, SaveKind::Knowledge),
        mode @ (SaveMode::Prepare | SaveMode::ApplyImmediately) => {
            let request = request.ok_or_else(|| {
                KbError::invalid_config(
                    "knowledge save request",
                    "a request is required unless confirming a prepared save",
                )
            })?;
            let preview = run_plan_create(context, vault.clone(), request)?;
            let operation_id = operation_id_from_preview(&preview)?;
            respond_to_prepared_save(
                context,
                vault,
                operation_id,
                &preview,
                &mode,
                SaveKind::Knowledge,
            )
        }
    }
}

#[derive(Clone, Copy)]
enum SaveKind {
    Source,
    Knowledge,
}

fn respond_to_prepared_save(
    context: &AppContext,
    vault: Option<String>,
    operation_id: Option<OperationId>,
    preview: &Value,
    mode: &SaveMode,
    kind: SaveKind,
) -> Result<Value, KbError> {
    let Some(operation_id) = operation_id else {
        return Ok(json!({
            "phase": "unchanged",
            "change_summary": Value::Null,
            "confirmation_token": Value::Null,
            "preview": preview,
            "result": Value::Null,
        }));
    };
    match mode {
        SaveMode::Prepare => Ok(json!({
            "phase": "awaiting_confirmation",
            "change_summary": change_summary(preview)?,
            "confirmation_token": operation_id,
            "preview": preview,
            "result": Value::Null,
        })),
        SaveMode::ApplyImmediately => confirm_save(context, vault, operation_id, kind),
        SaveMode::Confirm(_) => unreachable!("confirmation does not create a new preview"),
    }
}

fn confirm_save(
    context: &AppContext,
    vault: Option<String>,
    token: OperationId,
    expected_kind: SaveKind,
) -> Result<Value, KbError> {
    let selected = select_vault(context, vault)?;
    let state = inspect_operation(context.user_paths()?, token)?;
    if !matches_save_kind(&state, expected_kind) {
        return Err(KbError::invalid_config(
            "confirmation token",
            "token does not belong to the requested save kind",
        ));
    }
    let result = run_apply_for_resolved_vault(context, &selected, token)
        .map_err(|error| add_confirmation_token(error, token))?;
    let state = inspect_operation(context.user_paths()?, token)?;
    Ok(json!({
        "phase": "applied",
        "change_summary": change_summary_from_state(&state)?,
        "confirmation_token": Value::Null,
        "preview": Value::Null,
        "result": result,
    }))
}

fn operation_id_from_preview(preview: &Value) -> Result<Option<OperationId>, KbError> {
    match preview.get("operation_id") {
        Some(Value::String(value)) => value.parse().map(Some).map_err(|error| {
            KbError::invalid_config(
                "prepared operation",
                format!("invalid operation ID: {error}"),
            )
        }),
        Some(Value::Null) | None => Ok(None),
        _ => Err(KbError::invalid_config(
            "prepared operation",
            "operation ID must be a string or null",
        )),
    }
}

fn change_summary(preview: &Value) -> Result<Value, KbError> {
    let summary = preview
        .get("operation_summary")
        .and_then(Value::as_object)
        .ok_or_else(|| KbError::invalid_config("prepared save", "operation summary is missing"))?;
    let operation_kind = summary
        .get("operation_kind")
        .cloned()
        .unwrap_or(Value::Null);
    let (change_count, affected_paths, summary_text) = if operation_kind == "save_knowledge"
        && let Some(writes) = preview.get("writes").and_then(Value::as_array)
    {
        let paths = writes
            .iter()
            .map(|write| {
                write.get("path").cloned().ok_or_else(|| {
                    KbError::invalid_config("knowledge save preview", "write path is missing")
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let count = paths.len();
        (
            json!(count),
            Value::Array(paths),
            json!(format!("Save knowledge: {count} planned change(s).")),
        )
    } else {
        (
            summary.get("change_count").cloned().unwrap_or(Value::Null),
            summary
                .get("affected_paths")
                .cloned()
                .unwrap_or(Value::Null),
            summary.get("summary").cloned().unwrap_or(Value::Null),
        )
    };
    Ok(json!({
        "operation_kind": operation_kind,
        "change_count": change_count,
        "affected_paths": affected_paths,
        "summary": summary_text,
    }))
}

fn change_summary_from_state(state: &OperationState) -> Result<Value, KbError> {
    change_summary(&json!({ "operation_summary": summary_for_state(state) }))
}

fn matches_save_kind(state: &OperationState, expected: SaveKind) -> bool {
    matches!(
        (state, expected),
        (
            OperationState::PlannedSource(_) | OperationState::AppliedSource(_),
            SaveKind::Source
        ) | (
            OperationState::PlannedKnowledge(_) | OperationState::AppliedKnowledge(_),
            SaveKind::Knowledge
        )
    )
}

fn add_confirmation_token(mut error: KbError, token: OperationId) -> KbError {
    let details = match error.details.take() {
        Some(Value::Object(mut details)) => {
            details.insert("confirmation_token".to_owned(), json!(token));
            Value::Object(details)
        }
        Some(cause) => json!({ "confirmation_token": token, "cause": cause }),
        None => json!({ "confirmation_token": token }),
    };
    error.details = Some(details);
    error
}

fn run_plan_create(
    context: &AppContext,
    vault: Option<String>,
    request: kb_core::KnowledgePlanRequest,
) -> Result<Value, KbError> {
    let selected = select_vault(context, vault)?;
    ensure_mutation_allowed(&selected.root)?;
    let _lock = VaultLock::acquire(
        &selected.root,
        LockMode::Shared,
        "create knowledge plan",
        None,
    )?;
    crate::source_apply::ensure_no_pending(&selected.root)?;
    let config =
        crate::load_effective_config(&selected.root, context.user_paths()?, &context.overrides())?;
    let plan = crate::create_knowledge_plan(
        &selected.root,
        context.user_paths()?,
        &config,
        request,
        time::OffsetDateTime::now_utc(),
    )?;
    let summary = summary_for_knowledge_plan(&plan);
    crate::attach_operation_summary(to_value(plan)?, summary)
}

fn run_init(request: &InitRequest, context: &AppContext) -> Result<Value, KbError> {
    let report = match context.user_paths() {
        Ok(paths) => init_and_register_vault(request, paths)?,
        Err(error) => {
            let mut report = init_vault(request)?;
            report.warnings.push(format!(
                "Vault initialized but not registered: {error} Run kb vault register {}.",
                report.root.display()
            ));
            report
        }
    };
    to_value(report)
}

fn run_adopt(context: &AppContext, target: &std::path::Path) -> Result<Value, KbError> {
    let plan = create_adoption_plan(target, context.user_paths()?)?;
    let summary = summary_for_adoption_plan(&plan);
    crate::attach_operation_summary(to_value(plan)?, summary)
}

fn run_operation(request: OperationRequest, context: &AppContext) -> Result<Value, KbError> {
    match request {
        OperationRequest::Show { operation_id } => {
            let state = inspect_operation(context.user_paths()?, operation_id)?;
            let events = crate::operation_events(context.user_paths()?, operation_id)?;
            let latest_event = events
                .events
                .last()
                .expect("validated operation event reports are nonempty")
                .kind;
            let summary =
                crate::operation_summary::summary_for_state_with_event(&state, latest_event);
            let response = match state {
                OperationState::Planned(plan) => Ok(json!({ "state": "planned", "plan": plan })),
                OperationState::Applied(result) => {
                    Ok(json!({ "state": "applied", "result": result }))
                }
                OperationState::PlannedSource(plan) => Ok(json!({"state":"planned","plan":plan})),
                OperationState::AppliedSource(result) => {
                    Ok(json!({"state":"applied","result":result}))
                }
                OperationState::PlannedKnowledge(plan) => {
                    Ok(json!({"state":"planned","plan":plan}))
                }
                OperationState::AppliedKnowledge(result) => {
                    Ok(json!({"state":"applied","result":result}))
                }
                OperationState::PlannedSkill(plan) => Ok(json!({"state":"planned","plan":plan})),
                OperationState::AppliedSkill(result) => {
                    Ok(json!({"state":"applied","result":result}))
                }
            }?;
            crate::attach_operation_summary(response, summary)
        }
        OperationRequest::ShowForVault {
            vault,
            operation_id,
        } => {
            ensure_operation_vault(context, &vault, operation_id)?;
            run_operation(OperationRequest::Show { operation_id }, context)
        }
        OperationRequest::EventsForVault {
            vault,
            operation_id,
        } => {
            ensure_operation_vault(context, &vault, operation_id)?;
            to_value(crate::operation_events(
                context.user_paths()?,
                operation_id,
            )?)
        }
    }
}

fn ensure_operation_vault(
    context: &AppContext,
    vault: &str,
    operation_id: OperationId,
) -> Result<(), KbError> {
    let selected = select_vault(context, Some(vault.to_owned()))?;
    ensure_operation_for_resolved_vault(context, &selected, operation_id)
}

fn ensure_operation_for_resolved_vault(
    context: &AppContext,
    selected: &crate::ResolvedVault,
    operation_id: OperationId,
) -> Result<(), KbError> {
    let operation_vault_id = match inspect_operation(context.user_paths()?, operation_id)? {
        OperationState::Planned(value) => value.vault_id,
        OperationState::Applied(value) => value.vault_id,
        OperationState::PlannedSource(value) => value.vault_id,
        OperationState::AppliedSource(value) => value.vault_id,
        OperationState::PlannedKnowledge(value) => value.vault_id,
        OperationState::AppliedKnowledge(value) => value.vault_id,
        OperationState::PlannedSkill(value) => value.vault_id,
        OperationState::AppliedSkill(value) => value.vault_id,
    };
    if operation_vault_id != selected.vault_id {
        return Err(KbError::new(
            ErrorCode::AuthDenied,
            "Operation does not belong to the selected Vault.",
            false,
            "Use an operation created for the selected Vault.",
        ));
    }
    Ok(())
}

fn run_config(request: ConfigRequest, context: &AppContext) -> Result<Value, KbError> {
    let paths = context.user_paths()?;
    let overrides = context.overrides();
    match request {
        ConfigRequest::Show { vault, sources } => {
            let selected = select_vault(context, vault)?;
            let _lock = VaultLock::acquire(&selected.root, LockMode::Shared, "config show", None)?;
            config_show(&selected.root, paths, &overrides, sources)
        }
        ConfigRequest::Get { vault, key } => {
            let selected = select_vault(context, vault)?;
            let _lock = VaultLock::acquire(&selected.root, LockMode::Shared, "config get", None)?;
            config_get(&selected.root, paths, &overrides, &key)
        }
        ConfigRequest::Set {
            vault,
            target,
            key,
            value,
            write,
        } => {
            let selected = select_vault(context, vault)?;
            ensure_mutation_allowed(&selected.root)?;
            let _lock = VaultLock::acquire(
                &selected.root,
                if write {
                    LockMode::Exclusive
                } else {
                    LockMode::Shared
                },
                "config set",
                None,
            )?;
            if write {
                crate::source_apply::ensure_no_pending(&selected.root)?;
            }
            to_value(config_set(
                &selected.root,
                paths,
                &overrides,
                target,
                &key,
                &value,
                write,
            )?)
        }
        ConfigRequest::Unset {
            vault,
            target,
            key,
            write,
        } => {
            let selected = select_vault(context, vault)?;
            ensure_mutation_allowed(&selected.root)?;
            let _lock = VaultLock::acquire(
                &selected.root,
                if write {
                    LockMode::Exclusive
                } else {
                    LockMode::Shared
                },
                "config unset",
                None,
            )?;
            if write {
                crate::source_apply::ensure_no_pending(&selected.root)?;
            }
            to_value(config_unset(
                &selected.root,
                paths,
                &overrides,
                target,
                &key,
                write,
            )?)
        }
        ConfigRequest::Validate { vault } => {
            let selected = select_vault(context, vault)?;
            let _lock =
                VaultLock::acquire(&selected.root, LockMode::Shared, "config validate", None)?;
            to_value(config_validate(&selected.root, paths, &overrides)?)
        }
        ConfigRequest::Admission { vault, request } => {
            let selected = select_vault(context, vault)?;
            run_admission(&selected.root, request)
        }
    }
}

fn run_admission(root: &std::path::Path, request: AdmissionRequest) -> Result<Value, KbError> {
    match request {
        AdmissionRequest::List => {
            let _lock = VaultLock::acquire(root, LockMode::Shared, "config admission list", None)?;
            to_value(load_admission(root)?)
        }
        AdmissionRequest::Change { action, write } => {
            ensure_mutation_allowed(root)?;
            let mode = if write {
                LockMode::Exclusive
            } else {
                LockMode::Shared
            };
            let _lock = VaultLock::acquire(root, mode, "config admission change", None)?;
            if write {
                crate::source_apply::ensure_no_pending(root)?;
            }
            to_value(admission_change(root, &action, write)?)
        }
    }
}

fn run_vault(request: VaultRequest, context: &AppContext) -> Result<Value, KbError> {
    let paths = context.user_paths()?;
    match request {
        VaultRequest::List => Ok(json!({ "vaults": list_vaults(paths)? })),
        VaultRequest::Register { path } => to_value(register_vault(paths, &path)?),
        VaultRequest::Rebind { vault_id, path } => to_value(rebind_vault(paths, vault_id, &path)?),
        VaultRequest::Unregister { vault_id } => {
            unregister_vault(paths, vault_id)?;
            Ok(json!({ "vault_id": vault_id, "unregistered": true }))
        }
    }
}

fn run_skills(request: &SkillRequest, context: &AppContext) -> Result<Value, KbError> {
    let (vault, explicit_host, scope) = match &request {
        SkillRequest::Detect { vault } => (vault.clone(), None, SkillScope::Vault),
        SkillRequest::Install {
            vault, host, scope, ..
        }
        | SkillRequest::Status { vault, host, scope }
        | SkillRequest::Uninstall { vault, host, scope } => (vault.clone(), *host, *scope),
    };
    let selected = select_vault(context, vault)?;
    let detected = crate::detect_skill_hosts(&selected.root)?;
    if matches!(request, SkillRequest::Detect { .. }) {
        return Ok(json!({ "detected": detected }));
    }
    let host = resolve_skill_host(explicit_host, &detected)?;
    let roots = context.agent_roots()?;
    match request {
        SkillRequest::Status { .. } => to_value(crate::skill_status(
            context.user_paths()?,
            &selected.root,
            selected.vault_id,
            &roots,
            host,
            scope,
        )?),
        SkillRequest::Install { mode, .. } => {
            ensure_mutation_allowed(&selected.root)?;
            let _lock = VaultLock::acquire(
                &selected.root,
                LockMode::Shared,
                "create Skill install plan",
                None,
            )?;
            let plan = crate::create_skill_plan(&crate::SkillPlanRequest {
                vault_root: &selected.root,
                vault_id: selected.vault_id,
                user_paths: context.user_paths()?,
                roots: &roots,
                host,
                scope,
                mode: *mode,
                action: SkillAction::Install,
            })?;
            let summary = summary_for_skill_plan(&plan);
            crate::attach_operation_summary(to_value(plan)?, summary)
        }
        SkillRequest::Uninstall { .. } => {
            ensure_mutation_allowed(&selected.root)?;
            let target = skill_target(&selected.root, &roots, host, scope)?;
            let mode = match std::fs::symlink_metadata(target.skills_root.join("kb-vault")) {
                Ok(metadata) if metadata.file_type().is_symlink() => SkillInstallMode::Symlink,
                _ => SkillInstallMode::Copy,
            };
            let _lock = VaultLock::acquire(
                &selected.root,
                LockMode::Shared,
                "create Skill uninstall plan",
                None,
            )?;
            let plan = crate::create_skill_plan(&crate::SkillPlanRequest {
                vault_root: &selected.root,
                vault_id: selected.vault_id,
                user_paths: context.user_paths()?,
                roots: &roots,
                host,
                scope,
                mode,
                action: SkillAction::Uninstall,
            })?;
            let summary = summary_for_skill_plan(&plan);
            crate::attach_operation_summary(to_value(plan)?, summary)
        }
        SkillRequest::Detect { .. } => unreachable!("detect returned before host resolution"),
    }
}

fn run_paths(context: &AppContext, vault: Option<String>) -> Result<Value, KbError> {
    let selected = select_vault(context, vault)?;
    let _lock = VaultLock::acquire(&selected.root, LockMode::Shared, "paths", None)?;
    Ok(json!({
        "vault_id": selected.vault_id,
        "root": selected.root,
        "config": selected.root.join(".kb/config.yml"),
        "local_config": selected.root.join(".kb/config.local.yml"),
        "admission": selected.root.join("admission.yml"),
        "wiki": selected.root.join("Wiki"),
        "runtime": selected.root.join(".kb/runtime"),
        "cache": selected.root.join(".kb/cache"),
    }))
}

fn select_vault(
    context: &AppContext,
    explicit: Option<String>,
) -> Result<crate::ResolvedVault, KbError> {
    resolve_vault(
        context.user_paths()?,
        &VaultSelection {
            explicit,
            environment: context.environment.clone(),
            current_dir: context.current_dir.clone(),
        },
    )
}

fn select_doctor_root(context: &AppContext, explicit: Option<String>) -> Result<PathBuf, KbError> {
    let selector = explicit.or_else(|| context.environment.get("KB_VAULT").cloned());
    if let Some(selector) = selector {
        if Uuid::parse_str(&selector).is_ok() {
            return select_vault(context, Some(selector)).map(|vault| vault.root);
        }
        let path = PathBuf::from(selector);
        let root = if path.is_absolute() {
            path
        } else {
            context.current_dir.join(path)
        };
        kb_core::ensure_not_link_or_reparse_point(&root)?;
        if root.is_dir() {
            return Ok(root);
        }
        return Err(KbError::new(
            ErrorCode::VaultNotFound,
            format!("Vault directory does not exist: {}", root.display()),
            false,
            "Pass --vault with an existing Vault path.",
        ));
    }
    if let Ok(root) = kb_core::find_vault_root(&context.current_dir) {
        return Ok(root);
    }
    select_vault(context, None).map(|vault| vault.root)
}

pub(crate) fn ensure_mutation_allowed(root: &std::path::Path) -> Result<(), KbError> {
    let identity = crate::vault::read_vault_identity(root)?;
    match crate::schema::vault_schema_compatibility(identity.schema_version) {
        SchemaCompatibility::Current => Ok(()),
        SchemaCompatibility::OlderMigratable => {
            Err(migration_required_error(identity.schema_version))
        }
        SchemaCompatibility::OlderUnsupported => Err(KbError::new(
            ErrorCode::MigrationUnavailable,
            format!(
                "Vault schema {} has no migration path to {}.",
                identity.schema_version, CURRENT_SCHEMA_VERSION
            ),
            false,
            "Use a Knowledge-Brain version that explicitly supports this Vault schema, or preserve the Vault with an external byte-for-byte backup.",
        )),
        SchemaCompatibility::NewerMinorReadOnly | SchemaCompatibility::NewerMajorDiagnosticOnly => {
            Err(schema_too_new_error(identity.schema_version))
        }
    }
}

fn migration_required_error(version: kb_core::SchemaVersion) -> KbError {
    KbError::new(
        ErrorCode::MigrationRequired,
        format!("Vault schema {version} must be migrated before writing."),
        false,
        "Run the migration workflow with a compatible Knowledge-Brain version.",
    )
}

fn schema_too_new_error(version: kb_core::SchemaVersion) -> KbError {
    KbError::new(
        ErrorCode::SchemaTooNew,
        format!("Vault schema {version} is newer than supported {CURRENT_SCHEMA_VERSION}."),
        false,
        "Upgrade Knowledge-Brain; only status and doctor are available meanwhile.",
    )
}

fn to_value(value: impl Serialize) -> Result<Value, KbError> {
    serde_json::to_value(value)
        .map_err(|error| KbError::invalid_config("application response", error.to_string()))
}

fn run_apply(context: &AppContext, operation_id: kb_core::OperationId) -> Result<Value, KbError> {
    let operation = inspect_operation(context.user_paths()?, operation_id)?;
    match &operation {
        OperationState::PlannedSource(plan) => ensure_mutation_allowed(&plan.target)?,
        OperationState::AppliedSource(result) => ensure_mutation_allowed(&result.target)?,
        OperationState::PlannedKnowledge(plan) => ensure_mutation_allowed(&plan.target)?,
        OperationState::AppliedKnowledge(result) => ensure_mutation_allowed(&result.target)?,
        OperationState::Planned(_)
        | OperationState::Applied(_)
        | OperationState::PlannedSkill(_)
        | OperationState::AppliedSkill(_) => {}
    }
    match operation {
        OperationState::PlannedSource(_) | OperationState::AppliedSource(_) => {
            to_value(crate::source_apply::apply_capture(
                context.user_paths()?,
                operation_id,
                &context.overrides(),
            )?)
        }
        OperationState::PlannedKnowledge(_) | OperationState::AppliedKnowledge(_) => to_value(
            crate::apply_knowledge(context.user_paths()?, operation_id, &context.overrides())?,
        ),
        OperationState::PlannedSkill(_) | OperationState::AppliedSkill(_) => to_value(
            crate::apply_skill_plan(context.user_paths()?, &context.agent_roots()?, operation_id)?,
        ),
        _ => to_value(apply_operation(context.user_paths()?, operation_id)?),
    }
}

fn run_apply_for_vault(
    context: &AppContext,
    vault: &str,
    operation_id: OperationId,
) -> Result<Value, KbError> {
    let selected = select_vault(context, Some(vault.to_owned()))?;
    run_apply_for_resolved_vault(context, &selected, operation_id)
}

fn run_apply_for_resolved_vault(
    context: &AppContext,
    selected: &crate::ResolvedVault,
    operation_id: OperationId,
) -> Result<Value, KbError> {
    ensure_operation_for_resolved_vault(context, selected, operation_id)?;
    run_apply(context, operation_id)
}
