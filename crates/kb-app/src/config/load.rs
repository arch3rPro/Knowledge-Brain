use std::{collections::BTreeMap, fs, path::Path};

use kb_core::{
    CURRENT_SCHEMA_VERSION, ConfigSource, EffectiveConfig, KbError, PartialConfig, PartialFiles,
    PartialLimits, PartialOperations, PartialSearch, SearchMode,
};

use crate::UserPaths;

#[derive(Debug, Clone, Default)]
pub struct ConfigOverrides {
    pub environment: BTreeMap<String, String>,
    pub cli: BTreeMap<String, String>,
}

/// Load and merge every configuration layer in its defined precedence order.
///
/// # Errors
///
/// Returns [`KbError`] when a present file is unreadable or malformed, a schema
/// is unsupported, a required Vault identity is absent, or an override value is
/// invalid. Invalid higher-precedence values never fall through.
pub fn load_effective_config(
    vault_root: &Path,
    user_paths: &UserPaths,
    overrides: &ConfigOverrides,
) -> Result<EffectiveConfig, KbError> {
    let user = load_optional(
        &user_paths.config_dir.join("config.yml"),
        ConfigSource::User,
    )?;
    let vault = load_required(&vault_root.join(".kb/config.yml"), ConfigSource::Vault)?;
    let vault_id = vault.vault_id.ok_or_else(|| {
        KbError::invalid_config(
            ".kb/config.yml",
            "vault_id is required in Vault configuration",
        )
    })?;
    let local = load_optional(
        &vault_root.join(".kb/config.local.yml"),
        ConfigSource::VaultLocal,
    )?;

    reject_non_vault_identity(&user, "user config")?;
    reject_non_vault_identity(&local, ".kb/config.local.yml")?;

    let mut effective = EffectiveConfig::built_in(vault_id);
    effective.apply(user, ConfigSource::User);
    effective.apply(vault, ConfigSource::Vault);
    effective.apply(local, ConfigSource::VaultLocal);
    effective.apply(
        environment_layer(&overrides.environment)?,
        ConfigSource::Environment,
    );
    effective.apply(cli_layer(&overrides.cli)?, ConfigSource::Cli);
    validate_effective(&effective)?;
    Ok(effective)
}

fn load_required(path: &Path, source: ConfigSource) -> Result<PartialConfig, KbError> {
    if !path.is_file() {
        return Err(KbError::invalid_config(
            path.display().to_string(),
            "required configuration file is missing",
        ));
    }
    load_file(path, source)
}

fn load_optional(path: &Path, source: ConfigSource) -> Result<PartialConfig, KbError> {
    if !path.exists() {
        return Ok(PartialConfig::default());
    }
    load_file(path, source)
}

fn load_file(path: &Path, _source: ConfigSource) -> Result<PartialConfig, KbError> {
    let bytes = fs::read(path).map_err(|error| {
        KbError::io_failure("read", path.display().to_string(), error.to_string())
    })?;
    let parsed: PartialConfig = serde_yaml_ng::from_slice(&bytes)
        .map_err(|error| KbError::invalid_config(path.display().to_string(), error.to_string()))?;
    match parsed.schema_version {
        Some(version) if version == CURRENT_SCHEMA_VERSION => Ok(parsed),
        Some(version) => Err(KbError::invalid_config(
            path.display().to_string(),
            format!("schema {version} requires a compatibility workflow"),
        )),
        None => Err(KbError::invalid_config(
            path.display().to_string(),
            "schema_version is required",
        )),
    }
}

fn reject_non_vault_identity(config: &PartialConfig, path: &str) -> Result<(), KbError> {
    if config.vault_id.is_some() {
        return Err(KbError::invalid_config(
            path,
            "vault_id is owned by .kb/config.yml and cannot be overridden",
        ));
    }
    Ok(())
}

fn environment_layer(values: &BTreeMap<String, String>) -> Result<PartialConfig, KbError> {
    let mut mapped = BTreeMap::new();
    for (key, value) in values {
        let config_key = match key.as_str() {
            "KB_SEARCH_MODE" => Some("search.mode"),
            "KB_LIMITS_MAX_FILE_BYTES" => Some("limits.max_file_bytes"),
            "KB_LIMITS_MAX_FILES_PER_REVIEW" => Some("limits.max_files_per_review"),
            "KB_LIMITS_MAX_TOTAL_READ_BYTES" => Some("limits.max_total_read_bytes"),
            "KB_FILES_INCLUDE_HIDDEN" => Some("files.include_hidden"),
            "KB_OPERATIONS_PLAN_RETENTION_HOURS" => Some("operations.plan_retention_hours"),
            _ => None,
        };
        if let Some(config_key) = config_key {
            mapped.insert(config_key.to_owned(), value.clone());
        }
    }
    parse_override_map(&mapped, true)
}

fn cli_layer(values: &BTreeMap<String, String>) -> Result<PartialConfig, KbError> {
    parse_override_map(values, false)
}

fn parse_override_map(
    values: &BTreeMap<String, String>,
    environment: bool,
) -> Result<PartialConfig, KbError> {
    let mut partial = PartialConfig {
        schema_version: Some(CURRENT_SCHEMA_VERSION),
        ..PartialConfig::default()
    };
    for (key, value) in values {
        let display_key = if environment {
            environment_name(key)
        } else {
            key.clone()
        };
        match key.as_str() {
            "search.mode" => {
                partial
                    .search
                    .get_or_insert_with(PartialSearch::default)
                    .mode = Some(
                    value
                        .parse::<SearchMode>()
                        .map_err(|reason| invalid_override(&display_key, value, reason))?,
                );
            }
            "limits.max_file_bytes" => {
                partial
                    .limits
                    .get_or_insert_with(PartialLimits::default)
                    .max_file_bytes = Some(parse_positive_u64(&display_key, value)?);
            }
            "limits.max_files_per_review" => {
                partial
                    .limits
                    .get_or_insert_with(PartialLimits::default)
                    .max_files_per_review = Some(parse_positive_u64(&display_key, value)?);
            }
            "limits.max_total_read_bytes" => {
                partial
                    .limits
                    .get_or_insert_with(PartialLimits::default)
                    .max_total_read_bytes = Some(parse_positive_u64(&display_key, value)?);
            }
            "files.include_hidden" => {
                partial
                    .files
                    .get_or_insert_with(PartialFiles::default)
                    .include_hidden = Some(value.parse::<bool>().map_err(|_| {
                    invalid_override(&display_key, value, "expected true or false")
                })?);
            }
            "operations.plan_retention_hours" => {
                partial
                    .operations
                    .get_or_insert_with(PartialOperations::default)
                    .plan_retention_hours = Some(parse_positive_u64(&display_key, value)?);
            }
            _ => {
                return Err(KbError::invalid_config(
                    display_key,
                    "unknown command-line configuration key",
                ));
            }
        }
    }
    Ok(partial)
}

fn parse_positive_u64(key: &str, value: &str) -> Result<u64, KbError> {
    let parsed = value
        .parse::<u64>()
        .map_err(|_| invalid_override(key, value, "expected a positive integer"))?;
    if parsed == 0 {
        return Err(invalid_override(
            key,
            value,
            "value must be greater than zero",
        ));
    }
    Ok(parsed)
}

fn validate_effective(config: &EffectiveConfig) -> Result<(), KbError> {
    for (key, value) in [
        ("limits.max_file_bytes", config.limits.max_file_bytes.value),
        (
            "limits.max_files_per_review",
            config.limits.max_files_per_review.value,
        ),
        (
            "limits.max_total_read_bytes",
            config.limits.max_total_read_bytes.value,
        ),
        (
            "operations.plan_retention_hours",
            config.operations.plan_retention_hours.value,
        ),
    ] {
        if value == 0 {
            return Err(KbError::invalid_config(
                key,
                "value must be greater than zero",
            ));
        }
    }
    Ok(())
}

fn invalid_override(key: &str, value: &str, reason: &str) -> KbError {
    KbError::invalid_config(key, format!("invalid value {value:?}: {reason}"))
}

fn environment_name(key: &str) -> String {
    format!("KB_{}", key.replace('.', "_").to_uppercase())
}
