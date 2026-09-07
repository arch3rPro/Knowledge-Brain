use std::{collections::BTreeMap, path::PathBuf};

use kb_core::{CURRENT_SCHEMA_VERSION, ErrorCode, KbError, OperationId, SchemaCompatibility};
use serde::Serialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::{
    AdmissionAction, ConfigOverrides, ConfigTarget, InitRequest, LockMode, OperationState,
    UserPaths, VaultLock, VaultSelection, admission_change, apply_operation, capabilities,
    config_get, config_set, config_show, config_unset, config_validate, create_adoption_plan,
    doctor, init_and_register_vault, init_vault, inspect_operation, list_vaults, load_admission,
    rebind_vault, register_vault, resolve_vault, unregister_vault, vault_status,
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
}

#[derive(Debug, Clone)]
pub enum AppRequest {
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
    CacheRebuild {
        vault: Option<String>,
    },
    SourceVerify {
        vault: Option<String>,
    },
    Init(InitRequest),
    Adopt {
        target: PathBuf,
    },
    Apply {
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

#[derive(Debug, Clone, Copy)]
pub enum OperationRequest {
    Show { operation_id: OperationId },
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
            to_value(crate::review_sources(
                &selected.root,
                context.user_paths()?,
                &config,
            )?)
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
        AppRequest::Init(request) => run_init(&request, context),
        AppRequest::Adopt { target } => {
            to_value(create_adoption_plan(&target, context.user_paths()?)?)
        }
        AppRequest::Apply { operation_id } => run_apply(context, operation_id),
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
        AppRequest::Paths { vault } => {
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
        AppRequest::Version => Ok(json!({
            "app_version": env!("CARGO_PKG_VERSION"),
            "schema_version": CURRENT_SCHEMA_VERSION,
        })),
        AppRequest::Capabilities => to_value(capabilities()),
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

fn run_operation(request: OperationRequest, context: &AppContext) -> Result<Value, KbError> {
    match request {
        OperationRequest::Show { operation_id } => {
            match inspect_operation(context.user_paths()?, operation_id)? {
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
            }
        }
    }
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

fn ensure_mutation_allowed(root: &std::path::Path) -> Result<(), KbError> {
    let identity = crate::vault::read_vault_identity(root)?;
    match identity
        .schema_version
        .compatibility_with(CURRENT_SCHEMA_VERSION)
    {
        SchemaCompatibility::Current => Ok(()),
        SchemaCompatibility::OlderMigratable => Err(KbError::new(
            ErrorCode::MigrationRequired,
            format!(
                "Vault schema {} must be migrated before writing.",
                identity.schema_version
            ),
            false,
            "Run the migration workflow with a compatible Knowledge-Brain version.",
        )),
        SchemaCompatibility::NewerMinorReadOnly | SchemaCompatibility::NewerMajorDiagnosticOnly => {
            Err(KbError::new(
                ErrorCode::SchemaTooNew,
                format!(
                    "Vault schema {} is newer than supported {}.",
                    identity.schema_version, CURRENT_SCHEMA_VERSION
                ),
                false,
                "Upgrade Knowledge-Brain; only status and doctor are available meanwhile.",
            ))
        }
    }
}

fn to_value(value: impl Serialize) -> Result<Value, KbError> {
    serde_json::to_value(value)
        .map_err(|error| KbError::invalid_config("application response", error.to_string()))
}

fn run_apply(context: &AppContext, operation_id: kb_core::OperationId) -> Result<Value, KbError> {
    match inspect_operation(context.user_paths()?, operation_id)? {
        OperationState::PlannedSource(_) | OperationState::AppliedSource(_) => {
            to_value(crate::source_apply::apply_capture(
                context.user_paths()?,
                operation_id,
                &context.overrides(),
            )?)
        }
        OperationState::PlannedKnowledge(_) | OperationState::AppliedKnowledge(_) => {
            Err(KbError::new(
                ErrorCode::CapabilityUnavailable,
                "Knowledge apply is not available.",
                false,
                "Use a version that implements knowledge plan apply.",
            ))
        }
        _ => to_value(apply_operation(context.user_paths()?, operation_id)?),
    }
}
