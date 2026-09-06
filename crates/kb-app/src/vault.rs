use std::{collections::BTreeMap, fs, path::Path, path::PathBuf};

use kb_core::{
    CURRENT_SCHEMA_VERSION, ErrorCode, KbError, SchemaVersion, ensure_not_link_or_reparse_point,
    find_vault_root,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::{UserPaths, VaultRecord, list_vaults};

#[derive(Debug, Clone)]
pub struct VaultSelection {
    pub explicit: Option<String>,
    pub environment: BTreeMap<String, String>,
    pub current_dir: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ResolvedVault {
    pub vault_id: Uuid,
    pub root: PathBuf,
}

#[derive(Debug, Deserialize)]
struct VaultIdentity {
    schema_version: SchemaVersion,
    vault_id: Uuid,
}

/// Select a Vault deterministically from explicit, environment, directory, or
/// sole-registry context.
///
/// # Errors
///
/// Returns [`KbError`] when no unique usable Vault can be selected.
pub fn resolve_vault(
    user_paths: &UserPaths,
    selection: &VaultSelection,
) -> Result<ResolvedVault, KbError> {
    if let Some(selector) = &selection.explicit {
        return resolve_selector(user_paths, selector, &selection.current_dir);
    }
    if let Some(selector) = selection.environment.get("KB_VAULT") {
        if selector.trim().is_empty() {
            return Err(KbError::invalid_config("KB_VAULT", "value cannot be empty"));
        }
        return resolve_selector(user_paths, selector, &selection.current_dir);
    }
    if let Ok(root) = find_vault_root(&selection.current_dir) {
        return inspect_vault(&root).map(ResolvedVault::from);
    }

    let registered = list_vaults(user_paths)?;
    match registered.as_slice() {
        [only] => verify_record(only),
        [] => Err(vault_not_found(
            "No Vault selector or registration is available.",
        )),
        _ => Err(vault_not_found(
            "More than one Vault is registered, so none can be selected implicitly.",
        )),
    }
}

pub(crate) fn inspect_vault(root: &Path) -> Result<VaultRecord, KbError> {
    let absolute = absolute_path(root)?;
    ensure_not_link_or_reparse_point(&absolute)?;
    if !absolute.is_dir() {
        return Err(vault_not_found(&format!(
            "Vault directory does not exist: {}",
            absolute.display()
        )));
    }
    let config_path = absolute.join(".kb/config.yml");
    let bytes = fs::read(&config_path).map_err(|error| {
        KbError::io_failure("read", config_path.display().to_string(), error.to_string())
    })?;
    let identity: VaultIdentity = serde_yaml_ng::from_slice(&bytes).map_err(|error| {
        KbError::invalid_config(config_path.display().to_string(), error.to_string())
    })?;
    if identity.schema_version != CURRENT_SCHEMA_VERSION {
        return Err(KbError::invalid_config(
            config_path.display().to_string(),
            format!("unsupported schema version {}", identity.schema_version),
        ));
    }
    Ok(VaultRecord {
        vault_id: identity.vault_id,
        path: absolute,
    })
}

fn resolve_selector(
    user_paths: &UserPaths,
    selector: &str,
    current_dir: &Path,
) -> Result<ResolvedVault, KbError> {
    if let Ok(vault_id) = Uuid::parse_str(selector) {
        let record = list_vaults(user_paths)?
            .into_iter()
            .find(|record| record.vault_id == vault_id)
            .ok_or_else(|| vault_not_found(&format!("Vault {vault_id} is not registered.")))?;
        return verify_record(&record);
    }
    let path = Path::new(selector);
    let root = if path.is_absolute() {
        path.to_path_buf()
    } else {
        current_dir.join(path)
    };
    inspect_vault(&root).map(ResolvedVault::from)
}

fn verify_record(record: &VaultRecord) -> Result<ResolvedVault, KbError> {
    let actual = inspect_vault(&record.path).map_err(|error| {
        vault_not_found(&format!(
            "Registered Vault {} is unavailable at {}: {error}",
            record.vault_id,
            record.path.display()
        ))
    })?;
    if actual.vault_id != record.vault_id {
        return Err(vault_not_found(&format!(
            "Registered path {} now contains Vault {}, not {}.",
            record.path.display(),
            actual.vault_id,
            record.vault_id
        )));
    }
    Ok(actual.into())
}

fn absolute_path(path: &Path) -> Result<PathBuf, KbError> {
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }
    std::env::current_dir()
        .map(|current| current.join(path))
        .map_err(|error| {
            KbError::io_failure("resolve", path.display().to_string(), error.to_string())
        })
}

fn vault_not_found(message: &str) -> KbError {
    KbError::new(
        ErrorCode::VaultNotFound,
        message,
        false,
        "Pass --vault <path-or-id>, set KB_VAULT, or run kb vault list.",
    )
}

impl From<VaultRecord> for ResolvedVault {
    fn from(record: VaultRecord) -> Self {
        Self {
            vault_id: record.vault_id,
            root: record.path,
        }
    }
}
