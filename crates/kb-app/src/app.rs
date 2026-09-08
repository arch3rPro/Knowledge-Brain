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
        AppRequest::Review { vault } => {
            let selected = select_vault(context, vault)?;
            ensure_mutation_allowed(&selected.root)?;
            let _lock = VaultLock::acquire(&selected.root, LockMode::Shared, "review", None)?;
            crate::source_apply::ensure_no_pending(&selected.root)?;
            let config = crate::load_effective_config(
                &selected.root,
                context.user_paths()?,
                &context.overrides(),
            )?;
            let report = crate::review_sources(&selected.root, context.user_paths()?, &config)?;
            let response = to_value(&report)?;
            if let Some(operation_id) = report.operation_id {
                let state = inspect_operation(context.user_paths()?, operation_id)?;
                crate::attach_operation_summary(response, summary_for_state(&state))
            } else {
                Ok(response)
            }
        }
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
    ensure_operation_vault(context, vault, operation_id)?;
    run_apply(context, operation_id)
}
