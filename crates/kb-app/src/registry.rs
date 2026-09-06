use std::{fs, fs::File, path::Path};

use kb_core::{CURRENT_SCHEMA_VERSION, ErrorCode, KbError, SchemaVersion};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{UserPaths, atomic_replace};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultRegistry {
    pub schema_version: SchemaVersion,
    pub vaults: Vec<VaultRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VaultRecord {
    pub vault_id: Uuid,
    pub path: std::path::PathBuf,
}

impl Default for VaultRegistry {
    fn default() -> Self {
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            vaults: Vec::new(),
        }
    }
}

/// Return the machine-local Vault registrations.
///
/// # Errors
///
/// Returns [`KbError`] when the registry is unreadable or invalid.
pub fn list_vaults(user_paths: &UserPaths) -> Result<Vec<VaultRecord>, KbError> {
    Ok(load_registry(user_paths)?.vaults)
}

/// Register an initialized Vault without changing its contents.
///
/// # Errors
///
/// Returns [`KbError`] when the Vault identity or registry is invalid, a
/// conflicting registration exists, or the registry cannot be replaced.
pub fn register_vault(user_paths: &UserPaths, root: &Path) -> Result<VaultRecord, KbError> {
    let record = crate::vault::inspect_vault(root)?;
    with_registry_lock(user_paths, |registry| {
        if let Some(existing) = registry
            .vaults
            .iter()
            .find(|existing| existing.vault_id == record.vault_id)
        {
            if existing.path == record.path {
                return Ok(existing.clone());
            }
            return Err(KbError::new(
                ErrorCode::InvalidConfig,
                format!(
                    "Vault {} is already registered at {}.",
                    record.vault_id,
                    existing.path.display()
                ),
                false,
                format!(
                    "Run kb vault rebind {} {} if the Vault was moved.",
                    record.vault_id,
                    record.path.display()
                ),
            ));
        }
        reject_path_conflict(registry, &record)?;
        registry.vaults.push(record.clone());
        registry
            .vaults
            .sort_by_key(|registered| registered.vault_id);
        Ok(record)
    })
}

/// Point an existing stable Vault ID at its new location.
///
/// # Errors
///
/// Returns [`KbError`] when the new path has a different identity, the ID is
/// not registered, a path conflict exists, or the registry cannot be saved.
pub fn rebind_vault(
    user_paths: &UserPaths,
    vault_id: Uuid,
    root: &Path,
) -> Result<VaultRecord, KbError> {
    let candidate = crate::vault::inspect_vault(root)?;
    if candidate.vault_id != vault_id {
        return Err(KbError::invalid_config(
            root.display().to_string(),
            format!("expected vault_id {vault_id}, found {}", candidate.vault_id),
        ));
    }
    with_registry_lock(user_paths, |registry| {
        reject_path_conflict_for_id(registry, &candidate, vault_id)?;
        let existing = registry
            .vaults
            .iter_mut()
            .find(|record| record.vault_id == vault_id)
            .ok_or_else(|| unregistered(vault_id))?;
        existing.path.clone_from(&candidate.path);
        Ok(existing.clone())
    })
}

/// Remove one machine-local registration without deleting Vault data.
///
/// # Errors
///
/// Returns [`KbError`] when the registry cannot be read, locked, or saved.
pub fn unregister_vault(user_paths: &UserPaths, vault_id: Uuid) -> Result<(), KbError> {
    with_registry_lock(user_paths, |registry| {
        registry.vaults.retain(|record| record.vault_id != vault_id);
        Ok(())
    })
}

fn load_registry(user_paths: &UserPaths) -> Result<VaultRegistry, KbError> {
    let path = user_paths.config_dir.join("vaults.yml");
    if !path.exists() {
        return Ok(VaultRegistry::default());
    }
    let bytes = fs::read(&path).map_err(|error| {
        KbError::io_failure("read", path.display().to_string(), error.to_string())
    })?;
    let registry: VaultRegistry = serde_yaml_ng::from_slice(&bytes)
        .map_err(|error| KbError::invalid_config(path.display().to_string(), error.to_string()))?;
    if registry.schema_version != CURRENT_SCHEMA_VERSION {
        return Err(KbError::invalid_config(
            path.display().to_string(),
            format!("unsupported schema version {}", registry.schema_version),
        ));
    }
    validate_registry(&registry, &path)?;
    Ok(registry)
}

fn validate_registry(registry: &VaultRegistry, path: &Path) -> Result<(), KbError> {
    for (index, record) in registry.vaults.iter().enumerate() {
        if !record.path.is_absolute() {
            return Err(KbError::invalid_config(
                path.display().to_string(),
                format!("vaults[{index}].path must be absolute"),
            ));
        }
        if registry.vaults[..index]
            .iter()
            .any(|previous| previous.vault_id == record.vault_id || previous.path == record.path)
        {
            return Err(KbError::invalid_config(
                path.display().to_string(),
                format!("vaults[{index}] duplicates an earlier ID or path"),
            ));
        }
    }
    Ok(())
}

fn with_registry_lock<T>(
    user_paths: &UserPaths,
    update: impl FnOnce(&mut VaultRegistry) -> Result<T, KbError>,
) -> Result<T, KbError> {
    fs::create_dir_all(&user_paths.config_dir).map_err(|error| {
        KbError::io_failure(
            "create directory",
            user_paths.config_dir.display().to_string(),
            error.to_string(),
        )
    })?;
    fs::create_dir_all(&user_paths.state_dir).map_err(|error| {
        KbError::io_failure(
            "create directory",
            user_paths.state_dir.display().to_string(),
            error.to_string(),
        )
    })?;
    let lock_path = user_paths.state_dir.join("registry.lock");
    let lock = File::options()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&lock_path)
        .map_err(|error| {
            KbError::io_failure("open", lock_path.display().to_string(), error.to_string())
        })?;
    fs2::FileExt::lock_exclusive(&lock).map_err(|error| {
        KbError::io_failure("lock", lock_path.display().to_string(), error.to_string())
    })?;

    let mut registry = load_registry(user_paths)?;
    let result = update(&mut registry)?;
    validate_registry(&registry, &user_paths.config_dir.join("vaults.yml"))?;
    let output = serde_yaml_ng::to_string(&registry).map_err(|error| {
        KbError::invalid_config("Vault registry", format!("cannot serialize: {error}"))
    })?;
    atomic_replace(&user_paths.config_dir.join("vaults.yml"), output.as_bytes())?;
    Ok(result)
}

fn reject_path_conflict(registry: &VaultRegistry, candidate: &VaultRecord) -> Result<(), KbError> {
    reject_path_conflict_for_id(registry, candidate, candidate.vault_id)
}

fn reject_path_conflict_for_id(
    registry: &VaultRegistry,
    candidate: &VaultRecord,
    allowed_id: Uuid,
) -> Result<(), KbError> {
    if let Some(existing) = registry
        .vaults
        .iter()
        .find(|record| record.path == candidate.path && record.vault_id != allowed_id)
    {
        return Err(KbError::invalid_config(
            candidate.path.display().to_string(),
            format!("path is already registered as Vault {}", existing.vault_id),
        ));
    }
    Ok(())
}

fn unregistered(vault_id: Uuid) -> KbError {
    KbError::new(
        ErrorCode::VaultNotFound,
        format!("Vault {vault_id} is not registered."),
        false,
        "Run kb vault register <path> first.",
    )
}
