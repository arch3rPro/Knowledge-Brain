use std::{collections::BTreeMap, fs, path::Path};

use kb_core::{CURRENT_SCHEMA_VERSION, KbError, PortableRelativePath, SchemaCompatibility};
use serde::Serialize;

use crate::{
    ConfigOverrides, UserPaths, load_admission, load_effective_config, lock::probe_exclusive_lock,
    schema::vault_schema_compatibility, status::pending_operations,
};

#[derive(Debug, Clone, Serialize)]
pub struct DoctorReport {
    pub root: std::path::PathBuf,
    pub checks: Vec<DoctorCheck>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DoctorCheck {
    pub id: &'static str,
    pub status: CheckStatus,
    pub message: String,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckStatus {
    Pass,
    Warn,
    Fail,
    NotChecked,
}

/// Run independent, non-scored diagnostic checks.
///
/// This does not modify Vault data. The lock check may create and remove its
/// own empty file in `.kb/runtime`.
///
/// # Errors
///
/// Returns [`KbError`] only when the Vault identity itself cannot be read.
pub fn doctor(
    root: &Path,
    user_paths: &UserPaths,
    overrides: &ConfigOverrides,
) -> Result<DoctorReport, KbError> {
    let identity = crate::vault::read_vault_identity(root);
    let mut checks = vec![standard_directories(user_paths), vault_readability(root)];
    checks.push(vault_writability(root));
    checks.push(check_result("path_portability", check_portability(root)));
    checks.push(match identity {
        Ok(identity) => {
            let compatibility = vault_schema_compatibility(identity.schema_version);
            match compatibility {
                SchemaCompatibility::Current => check_result(
                    "configuration",
                    check_configuration(root, user_paths, overrides),
                ),
                SchemaCompatibility::OlderUnsupported => DoctorCheck {
                    id: "configuration",
                    status: CheckStatus::Warn,
                    message: format!(
                        "Schema {} has no migration path to {}; current fields were not interpreted.",
                        identity.schema_version, CURRENT_SCHEMA_VERSION
                    ),
                },
                SchemaCompatibility::OlderMigratable => DoctorCheck {
                    id: "configuration",
                    status: CheckStatus::Warn,
                    message: format!(
                        "Schema {} has a migration path to {}; current fields were not interpreted.",
                        identity.schema_version, CURRENT_SCHEMA_VERSION
                    ),
                },
                SchemaCompatibility::NewerMinorReadOnly
                | SchemaCompatibility::NewerMajorDiagnosticOnly => DoctorCheck {
                    id: "configuration",
                    status: CheckStatus::Warn,
                    message: format!(
                        "Schema {} is {compatibility:?}; current fields were not interpreted.",
                        identity.schema_version
                    ),
                },
            }
        }
        Err(error) => DoctorCheck {
            id: "configuration",
            status: CheckStatus::Fail,
            message: error.to_string(),
        },
    });
    checks.push(check_result("lock_acquisition", probe_exclusive_lock(root)));
    let pending = pending_operations(user_paths, root);
    checks.push(DoctorCheck {
        id: "recovery_records",
        status: if pending == 0 {
            CheckStatus::Pass
        } else {
            CheckStatus::Warn
        },
        message: format!("{pending} operation recovery record(s) are pending."),
    });
    checks.push(DoctorCheck {
        id: "optional_integrations",
        status: CheckStatus::NotChecked,
        message: "No optional host integration is configured in Stage 1.".to_owned(),
    });
    Ok(DoctorReport {
        root: root.to_path_buf(),
        checks,
    })
}

fn standard_directories(user_paths: &UserPaths) -> DoctorCheck {
    let paths = [
        &user_paths.config_dir,
        &user_paths.state_dir,
        &user_paths.cache_dir,
    ];
    let absolute = paths.iter().all(|path| path.is_absolute());
    let existing = paths.iter().filter(|path| path.is_dir()).count();
    DoctorCheck {
        id: "standard_directories",
        status: if absolute && existing == paths.len() {
            CheckStatus::Pass
        } else if absolute {
            CheckStatus::Warn
        } else {
            CheckStatus::Fail
        },
        message: format!(
            "{existing}/{} standard directories currently exist.",
            paths.len()
        ),
    }
}

fn vault_readability(root: &Path) -> DoctorCheck {
    check_result(
        "vault_readability",
        fs::read_dir(root).map(|_| ()).map_err(|error| {
            KbError::io_failure("read", root.display().to_string(), error.to_string())
        }),
    )
}

fn vault_writability(root: &Path) -> DoctorCheck {
    match fs::metadata(root) {
        Ok(metadata) if metadata.permissions().readonly() => DoctorCheck {
            id: "vault_writability",
            status: CheckStatus::Warn,
            message: "Vault root is marked read-only.".to_owned(),
        },
        Ok(_) => DoctorCheck {
            id: "vault_writability",
            status: CheckStatus::Pass,
            message: "Vault root is not marked read-only.".to_owned(),
        },
        Err(error) => DoctorCheck {
            id: "vault_writability",
            status: CheckStatus::Fail,
            message: error.to_string(),
        },
    }
}

fn check_configuration(
    root: &Path,
    user_paths: &UserPaths,
    overrides: &ConfigOverrides,
) -> Result<(), KbError> {
    load_effective_config(root, user_paths, overrides)?;
    load_admission(root)?;
    Ok(())
}

fn check_portability(root: &Path) -> Result<(), KbError> {
    fn visit(
        root: &Path,
        directory: &Path,
        seen: &mut BTreeMap<String, String>,
    ) -> Result<(), KbError> {
        let entries = fs::read_dir(directory).map_err(|error| {
            KbError::io_failure("read", directory.display().to_string(), error.to_string())
        })?;
        for entry in entries {
            let entry = entry.map_err(|error| {
                KbError::io_failure("read", directory.display().to_string(), error.to_string())
            })?;
            let path = entry.path();
            kb_core::ensure_not_link_or_reparse_point(&path)?;
            let relative =
                PortableRelativePath::from_path(path.strip_prefix(root).map_err(|error| {
                    KbError::invalid_config(path.display().to_string(), error.to_string())
                })?)?;
            let key = kb_core::portability_key(&relative);
            if let Some(previous) = seen.insert(key, relative.as_str().to_owned()) {
                return Err(KbError::invalid_config(
                    "Vault paths",
                    format!("{previous} collides with {}", relative.as_str()),
                ));
            }
            if path.is_dir() {
                visit(root, &path, seen)?;
            }
        }
        Ok(())
    }
    visit(root, root, &mut BTreeMap::new())
}

fn check_result(id: &'static str, result: Result<(), KbError>) -> DoctorCheck {
    match result {
        Ok(()) => DoctorCheck {
            id,
            status: CheckStatus::Pass,
            message: "Check passed.".to_owned(),
        },
        Err(error) => DoctorCheck {
            id,
            status: CheckStatus::Fail,
            message: error.to_string(),
        },
    }
}
