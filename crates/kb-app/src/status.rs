use std::{fs, path::Path};

use kb_core::{CURRENT_SCHEMA_VERSION, KbError, SchemaCompatibility, SchemaVersion};
use serde::Serialize;
use uuid::Uuid;

use crate::{ConfigOverrides, UserPaths, load_admission, load_effective_config};

#[derive(Debug, Clone, Serialize)]
pub struct StatusReport {
    pub root: std::path::PathBuf,
    pub vault_id: Uuid,
    pub schema: SchemaStatus,
    pub configuration: ValidationState,
    pub admission: AdmissionStatus,
    pub recovery: RecoveryStatus,
    pub cache: CacheStatus,
}

#[derive(Debug, Clone, Serialize)]
pub struct SchemaStatus {
    pub version: SchemaVersion,
    pub compatibility: SchemaCompatibility,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ValidationState {
    Valid,
    NotInterpreted,
}

#[derive(Debug, Clone, Serialize)]
pub struct AdmissionStatus {
    pub enabled: Option<usize>,
    pub disabled: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RecoveryStatus {
    pub pending_operations: usize,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CacheStatus {
    Missing,
    Empty,
    Present,
}

/// Report factual Vault state without assigning an aggregate score.
///
/// # Errors
///
/// Returns [`KbError`] when a current schema's configuration or admission list
/// is invalid, or required diagnostic metadata cannot be read.
pub fn vault_status(
    root: &Path,
    user_paths: &UserPaths,
    overrides: &ConfigOverrides,
) -> Result<StatusReport, KbError> {
    let identity = crate::vault::read_vault_identity(root)?;
    let compatibility = identity
        .schema_version
        .compatibility_with(CURRENT_SCHEMA_VERSION);
    let (configuration, admission) = if compatibility == SchemaCompatibility::Current {
        load_effective_config(root, user_paths, overrides)?;
        let admission = load_admission(root)?;
        let enabled = admission
            .directories
            .iter()
            .filter(|entry| entry.enabled)
            .count();
        (
            ValidationState::Valid,
            AdmissionStatus {
                enabled: Some(enabled),
                disabled: Some(admission.directories.len() - enabled),
            },
        )
    } else {
        (
            ValidationState::NotInterpreted,
            AdmissionStatus {
                enabled: None,
                disabled: None,
            },
        )
    };

    Ok(StatusReport {
        root: root.to_path_buf(),
        vault_id: identity.vault_id,
        schema: SchemaStatus {
            version: identity.schema_version,
            compatibility,
        },
        configuration,
        admission,
        recovery: RecoveryStatus {
            pending_operations: pending_operations(user_paths, root),
        },
        cache: cache_status(root)?,
    })
}

pub(crate) fn pending_operations(user_paths: &UserPaths, root: &Path) -> usize {
    let operations = user_paths.state_dir.join("operations");
    let Ok(entries) = fs::read_dir(operations) else {
        return 0;
    };
    entries
        .filter_map(Result::ok)
        .filter(|entry| entry.path().join("progress.json").is_file())
        .filter(|entry| {
            let plan = entry.path().join("plan.json");
            fs::read(plan)
                .ok()
                .and_then(|bytes| serde_json::from_slice::<kb_core::AdoptionPlan>(&bytes).ok())
                .is_some_and(|plan| plan.target == root)
        })
        .count()
}

fn cache_status(root: &Path) -> Result<CacheStatus, KbError> {
    let cache = root.join(".kb/cache");
    if !cache.exists() {
        return Ok(CacheStatus::Missing);
    }
    let mut entries = fs::read_dir(&cache).map_err(|error| {
        KbError::io_failure("read", cache.display().to_string(), error.to_string())
    })?;
    Ok(if entries.next().is_some() {
        CacheStatus::Present
    } else {
        CacheStatus::Empty
    })
}
