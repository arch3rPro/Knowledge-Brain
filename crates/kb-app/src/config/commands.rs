use std::{
    fs,
    path::{Path, PathBuf},
};

use kb_core::{ConfigSource, EffectiveConfig, KbError, SearchMode};
use serde::Serialize;
use serde_json::{Value, json};

use crate::{ConfigOverrides, UserPaths, atomic_replace, load_effective_config};

use super::{
    AdmissionAction, edit_admission, edit_config,
    load::{load_effective_config_replacing, parse_config_text},
    load_admission,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigTarget {
    Vault,
    Local,
    User,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConfigChange {
    pub path: PathBuf,
    pub diff: String,
    pub written: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ValidationReport {
    pub valid: bool,
    pub admission_entries: usize,
}

/// Return effective configuration, optionally retaining field provenance.
///
/// # Errors
///
/// Returns `KbError` when any participating configuration layer is invalid.
pub fn config_show(
    vault_root: &Path,
    user_paths: &UserPaths,
    overrides: &ConfigOverrides,
    sources: bool,
) -> Result<Value, KbError> {
    let config = load_effective_config(vault_root, user_paths, overrides)?;
    if sources {
        serde_json::to_value(config)
            .map_err(|error| KbError::invalid_config("effective configuration", error.to_string()))
    } else {
        Ok(values_only(&config))
    }
}

/// Return one effective configuration value and its source.
///
/// # Errors
///
/// Returns `KbError` when configuration is invalid or the key is unknown.
pub fn config_get(
    vault_root: &Path,
    user_paths: &UserPaths,
    overrides: &ConfigOverrides,
    key: &str,
) -> Result<Value, KbError> {
    let config = load_effective_config(vault_root, user_paths, overrides)?;
    let (value, source) = get_value(&config, key)?;
    Ok(json!({ "key": key, "value": value, "source": source }))
}

/// Preview or save a scalar configuration value in one selected layer.
///
/// # Errors
///
/// Returns `KbError` if the target layer, current text, candidate text, or final
/// effective value is invalid, or if the atomic replacement fails.
pub fn config_set(
    vault_root: &Path,
    user_paths: &UserPaths,
    overrides: &ConfigOverrides,
    target: ConfigTarget,
    key: &str,
    value: &str,
    write: bool,
) -> Result<ConfigChange, KbError> {
    change_config(
        vault_root,
        user_paths,
        overrides,
        target,
        key,
        Some(value),
        write,
    )
}

/// Preview or remove a scalar configuration value in one selected layer.
///
/// # Errors
///
/// Returns `KbError` under the same conditions as `config_set`.
pub fn config_unset(
    vault_root: &Path,
    user_paths: &UserPaths,
    overrides: &ConfigOverrides,
    target: ConfigTarget,
    key: &str,
    write: bool,
) -> Result<ConfigChange, KbError> {
    change_config(vault_root, user_paths, overrides, target, key, None, write)
}

/// Parse every configuration source and validate admission boundaries.
///
/// # Errors
///
/// Returns `KbError` for any invalid configuration or admission entry.
pub fn config_validate(
    vault_root: &Path,
    user_paths: &UserPaths,
    overrides: &ConfigOverrides,
) -> Result<ValidationReport, KbError> {
    load_effective_config(vault_root, user_paths, overrides)?;
    let admission = load_admission(vault_root)?;
    Ok(ValidationReport {
        valid: true,
        admission_entries: admission.directories.len(),
    })
}

/// Preview or save one admission-list operation.
///
/// # Errors
///
/// Returns `KbError` when the current or candidate admission document is invalid
/// or the file cannot be atomically replaced.
pub fn admission_change(
    vault_root: &Path,
    action: &AdmissionAction,
    write: bool,
) -> Result<ConfigChange, KbError> {
    let path = vault_root.join("admission.yml");
    let before = fs::read_to_string(&path).map_err(|error| {
        KbError::io_failure("read", path.display().to_string(), error.to_string())
    })?;
    let after = edit_admission(&before, vault_root, action)?;
    let change = ConfigChange {
        path: path.clone(),
        diff: line_diff(&path, &before, &after),
        written: write,
    };
    if write {
        atomic_replace(&path, after.as_bytes())?;
    }
    Ok(change)
}

fn change_config(
    vault_root: &Path,
    user_paths: &UserPaths,
    overrides: &ConfigOverrides,
    target: ConfigTarget,
    key: &str,
    value: Option<&str>,
    write: bool,
) -> Result<ConfigChange, KbError> {
    let path = target_path(vault_root, user_paths, target);
    let before = if path.exists() {
        fs::read_to_string(&path).map_err(|error| {
            KbError::io_failure("read", path.display().to_string(), error.to_string())
        })?
    } else {
        "schema_version: \"v1.0\"\n".to_owned()
    };
    let after = edit_config(&before, key, value)?;
    let candidate = parse_config_text(&after, &path.display().to_string())?;
    match target {
        ConfigTarget::Vault if candidate.vault_id.is_none() => {
            return Err(KbError::invalid_config(
                path.display().to_string(),
                "vault_id is required in Vault configuration",
            ));
        }
        ConfigTarget::Local | ConfigTarget::User if candidate.vault_id.is_some() => {
            return Err(KbError::invalid_config(
                path.display().to_string(),
                "vault_id cannot be overridden outside .kb/config.yml",
            ));
        }
        ConfigTarget::Vault | ConfigTarget::Local | ConfigTarget::User => {}
    }
    load_effective_config_replacing(
        vault_root,
        user_paths,
        overrides,
        target.source(),
        candidate,
    )?;

    let change = ConfigChange {
        path: path.clone(),
        diff: line_diff(&path, &before, &after),
        written: write,
    };
    if write {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                KbError::io_failure(
                    "create directory",
                    parent.display().to_string(),
                    error.to_string(),
                )
            })?;
        }
        atomic_replace(&path, after.as_bytes())?;
    }
    Ok(change)
}

impl ConfigTarget {
    const fn source(self) -> ConfigSource {
        match self {
            Self::Vault => ConfigSource::Vault,
            Self::Local => ConfigSource::VaultLocal,
            Self::User => ConfigSource::User,
        }
    }
}

fn target_path(vault_root: &Path, user_paths: &UserPaths, target: ConfigTarget) -> PathBuf {
    match target {
        ConfigTarget::Vault => vault_root.join(".kb/config.yml"),
        ConfigTarget::Local => vault_root.join(".kb/config.local.yml"),
        ConfigTarget::User => user_paths.config_dir.join("config.yml"),
    }
}

fn get_value(config: &EffectiveConfig, key: &str) -> Result<(Value, ConfigSource), KbError> {
    match key {
        "search.mode" => Ok((
            Value::String(config.search.mode.value.to_string()),
            config.search.mode.source,
        )),
        "limits.max_file_bytes" => Ok((
            json!(config.limits.max_file_bytes.value),
            config.limits.max_file_bytes.source,
        )),
        "limits.max_files_per_review" => Ok((
            json!(config.limits.max_files_per_review.value),
            config.limits.max_files_per_review.source,
        )),
        "limits.max_total_read_bytes" => Ok((
            json!(config.limits.max_total_read_bytes.value),
            config.limits.max_total_read_bytes.source,
        )),
        "files.include_hidden" => Ok((
            json!(config.files.include_hidden.value),
            config.files.include_hidden.source,
        )),
        "operations.plan_retention_hours" => Ok((
            json!(config.operations.plan_retention_hours.value),
            config.operations.plan_retention_hours.source,
        )),
        _ => Err(KbError::invalid_config(key, "unknown configuration key")),
    }
}

fn values_only(config: &EffectiveConfig) -> Value {
    json!({
        "schema_version": config.schema_version,
        "vault_id": config.vault_id,
        "search": { "mode": match config.search.mode.value {
            SearchMode::Direct => "direct",
            SearchMode::Bm25 => "bm25",
        }},
        "limits": {
            "max_file_bytes": config.limits.max_file_bytes.value,
            "max_files_per_review": config.limits.max_files_per_review.value,
            "max_total_read_bytes": config.limits.max_total_read_bytes.value,
        },
        "files": { "include_hidden": config.files.include_hidden.value },
        "operations": { "plan_retention_hours": config.operations.plan_retention_hours.value },
    })
}

fn line_diff(path: &Path, before: &str, after: &str) -> String {
    let mut diff = format!("--- {}\n+++ {}\n", path.display(), path.display());
    for line in before.lines() {
        diff.push('-');
        diff.push_str(line);
        diff.push('\n');
    }
    for line in after.lines() {
        diff.push('+');
        diff.push_str(line);
        diff.push('\n');
    }
    diff
}
