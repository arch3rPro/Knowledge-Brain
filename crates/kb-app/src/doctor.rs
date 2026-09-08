use std::{
    collections::BTreeMap,
    fs,
    io::{ErrorKind, Write},
    path::Path,
};

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
/// This does not modify Vault data. When `.kb/runtime` already exists and is
/// valid, the lock check may create and remove its own empty lock file there.
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
    let mut checks = vec![
        machine_runtime_directories(user_paths),
        vault_readability(root),
        vault_structure(root),
    ];
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
    checks.push(lock_acquisition(root));
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

fn machine_runtime_directories(user_paths: &UserPaths) -> DoctorCheck {
    let directories = [
        ("configuration", &user_paths.config_dir),
        ("state", &user_paths.state_dir),
        ("cache", &user_paths.cache_dir),
    ];
    let mut missing = 0;
    let mut warnings = Vec::new();
    let mut failures = Vec::new();

    for (purpose, path) in directories {
        match fs::metadata(path) {
            Ok(metadata) if !metadata.is_dir() => failures.push(format!(
                "{purpose} path is not a directory: {}",
                path.display()
            )),
            Ok(_) => {
                if let Err(error) = fs::read_dir(path) {
                    failures.push(format!(
                        "cannot inspect {purpose} directory {}: {error}",
                        path.display()
                    ));
                } else if let Err(error) = probe_directory_write(path) {
                    warnings.push(format!(
                        "cannot write to {purpose} directory {}: {error}",
                        path.display()
                    ));
                }
            }
            Err(error) if error.kind() == ErrorKind::NotFound => missing += 1,
            Err(error) => failures.push(format!(
                "cannot inspect {purpose} directory {}: {error}",
                path.display()
            )),
        }
    }

    let locations = directories
        .iter()
        .map(|(purpose, path)| format!("{purpose}: {}", path.display()))
        .collect::<Vec<_>>()
        .join("; ");
    let message = format!(
        "Machine runtime directories ({locations}) are created on demand; {missing} path(s) are currently absent."
    );

    DoctorCheck {
        id: "machine_runtime_directories",
        status: if !failures.is_empty() {
            CheckStatus::Fail
        } else if !warnings.is_empty() {
            CheckStatus::Warn
        } else {
            CheckStatus::Pass
        },
        message: [message]
            .into_iter()
            .chain(failures)
            .chain(warnings)
            .collect::<Vec<_>>()
            .join(" "),
    }
}

fn probe_directory_write(path: &Path) -> std::io::Result<()> {
    let mut probe = tempfile::Builder::new()
        .prefix(".kb-doctor-write-probe-")
        .tempfile_in(path)?;
    probe.write_all(b"kb doctor write probe")
}

fn lock_acquisition(root: &Path) -> DoctorCheck {
    let runtime = root.join(".kb/runtime");
    match fs::symlink_metadata(&runtime) {
        Ok(metadata) if metadata.is_dir() => {
            check_result("lock_acquisition", probe_exclusive_lock(root))
        }
        Ok(_) => DoctorCheck {
            id: "lock_acquisition",
            status: CheckStatus::NotChecked,
            message: format!(
                "Lock probe skipped because Vault runtime path is not a directory: {}.",
                runtime.display()
            ),
        },
        Err(error) if error.kind() == ErrorKind::NotFound => DoctorCheck {
            id: "lock_acquisition",
            status: CheckStatus::NotChecked,
            message: format!(
                "Lock probe skipped because Vault runtime directory is missing: {}.",
                runtime.display()
            ),
        },
        Err(error) => DoctorCheck {
            id: "lock_acquisition",
            status: CheckStatus::Fail,
            message: format!(
                "Cannot inspect Vault runtime directory {}: {error}",
                runtime.display()
            ),
        },
    }
}

fn vault_structure(root: &Path) -> DoctorCheck {
    let required = [
        ("admission.yml", RequiredPathKind::File),
        ("KB.md", RequiredPathKind::File),
        ("Wiki", RequiredPathKind::Directory),
        ("Wiki/index.md", RequiredPathKind::File),
        ("Wiki/log.md", RequiredPathKind::File),
        (
            "Wiki/external-sources/.objects/sha256",
            RequiredPathKind::Directory,
        ),
        ("Wiki/research", RequiredPathKind::Directory),
        ("Wiki/articles", RequiredPathKind::Directory),
        (".kb", RequiredPathKind::Directory),
        (".kb/config.yml", RequiredPathKind::File),
        (".kb/schemas/admission.schema.json", RequiredPathKind::File),
        (".kb/schemas/config.schema.json", RequiredPathKind::File),
        (".kb/cache", RequiredPathKind::Directory),
        (".kb/runtime", RequiredPathKind::Directory),
    ];
    let invalid = required
        .into_iter()
        .filter_map(|(relative, expected)| {
            let path = root.join(relative);
            match fs::symlink_metadata(&path) {
                Ok(metadata) if metadata.file_type().is_symlink() => {
                    Some(format!("{relative} is a symbolic link"))
                }
                Ok(metadata) if !expected.matches(&metadata) => {
                    Some(format!("{relative} is not a {}", expected.description()))
                }
                Ok(_) => None,
                Err(error) if error.kind() == ErrorKind::NotFound => {
                    Some(format!("{relative} is missing"))
                }
                Err(error) => Some(format!("cannot inspect {relative}: {error}")),
            }
        })
        .collect::<Vec<_>>();

    DoctorCheck {
        id: "vault_structure",
        status: if invalid.is_empty() {
            CheckStatus::Pass
        } else {
            CheckStatus::Fail
        },
        message: if invalid.is_empty() {
            "All required Vault paths are present with the expected types.".to_owned()
        } else {
            format!("Vault structure findings: {}.", invalid.join("; "))
        },
    }
}

#[derive(Clone, Copy)]
enum RequiredPathKind {
    File,
    Directory,
}

impl RequiredPathKind {
    fn matches(self, metadata: &fs::Metadata) -> bool {
        match self {
            Self::File => metadata.is_file(),
            Self::Directory => metadata.is_dir(),
        }
    }

    const fn description(self) -> &'static str {
        match self {
            Self::File => "file",
            Self::Directory => "directory",
        }
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
