use std::{fs, path::Path};

use kb_core::{KbError, SchemaCompatibility, SchemaVersion};
use serde::Serialize;
use uuid::Uuid;

use crate::{
    ConfigOverrides, UserPaths, load_admission, load_effective_config,
    schema::vault_schema_compatibility,
    template_state::{TemplateInspection, inspect_template},
};

#[derive(Debug, Clone, Serialize)]
pub struct StatusReport {
    pub root: std::path::PathBuf,
    pub vault_id: Uuid,
    pub rules_uri: String,
    pub wiki_index_uri: String,
    pub schema: SchemaStatus,
    pub template: TemplateInspection,
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
    let compatibility = vault_schema_compatibility(identity.schema_version);
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
        rules_uri: format!("kb-vault://{}/KB.md", identity.vault_id),
        wiki_index_uri: format!("kb-vault://{}/Wiki/index.md", identity.vault_id),
        schema: SchemaStatus {
            version: identity.schema_version,
            compatibility,
        },
        template: inspect_template(root)?,
        configuration,
        admission,
        recovery: RecoveryStatus {
            pending_operations: pending_operations(user_paths, root),
        },
        cache: cache_status(root)?,
    })
}

pub(crate) fn pending_operations(user_paths: &UserPaths, root: &Path) -> usize {
    let source_pending = usize::from(root.join(".kb/runtime/source-pending.json").exists());
    let knowledge_pending = usize::from(root.join(".kb/runtime/knowledge-pending.json").exists());
    let upgrade_pending = usize::from(root.join(".kb/runtime/vault-upgrade-pending.json").exists());
    let update_pending = incomplete_update_operations(user_paths, root);
    let operations = user_paths.state_dir.join("operations");
    let Ok(entries) = fs::read_dir(operations) else {
        return source_pending + knowledge_pending + upgrade_pending + update_pending;
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
        + source_pending
        + knowledge_pending
        + upgrade_pending
        + update_pending
}

pub(crate) fn incomplete_update_operations(user_paths: &UserPaths, root: &Path) -> usize {
    let Ok(identity) = crate::vault::read_vault_identity(root) else {
        return 0;
    };
    let updates = user_paths.state_dir.join("updates");
    let Ok(entries) = fs::read_dir(updates) else {
        return 0;
    };
    entries
        .filter_map(Result::ok)
        .filter_map(|entry| fs::read(entry.path().join("operation.json")).ok())
        .filter_map(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
        .filter(|value| {
            let state = value
                .get("execution_state")
                .and_then(serde_json::Value::as_str);
            let terminal = matches!(
                state,
                Some("completed" | "completed_with_skips" | "partial" | "failed")
            );
            let contains_vault = value
                .pointer("/scope/vaults")
                .and_then(serde_json::Value::as_array)
                .is_some_and(|vaults| {
                    vaults
                        .iter()
                        .any(|vault| vault.as_str() == Some(&identity.vault_id.to_string()))
                });
            !terminal && contains_vault
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
