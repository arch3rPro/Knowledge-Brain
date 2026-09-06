use std::{
    fs,
    path::{Path, PathBuf},
};

use kb_core::{ErrorCode, KbError, ensure_not_link_or_reparse_point};
use serde::Serialize;
use uuid::Uuid;

use crate::{
    storage::atomic_replace,
    template::{EMPTY_DIRECTORIES, STATIC_FILES, config_yaml},
};

#[derive(Debug, Clone)]
pub struct InitRequest {
    pub target: PathBuf,
}

#[derive(Debug, Clone, Serialize)]
pub struct InitReport {
    pub vault_id: Uuid,
    pub root: PathBuf,
    pub created_files: Vec<String>,
    pub warnings: Vec<String>,
}

/// Create a minimum Vault in a nonexistent or empty directory.
///
/// # Errors
///
/// Returns [`KbError`] without changing a nonempty target, link-shaped target,
/// or target whose staged template cannot be fully written and validated.
pub fn init_vault(request: &InitRequest) -> Result<InitReport, KbError> {
    let target = absolute_path(&request.target)?;
    let vault_id = Uuid::new_v4();

    let warnings = if target.exists() {
        ensure_not_link_or_reparse_point(&target)?;
        let metadata =
            fs::metadata(&target).map_err(|error| io_error("inspect", &target, &error))?;
        if !metadata.is_dir() {
            return Err(KbError::new(
                ErrorCode::TargetNotEmpty,
                format!(
                    "Initialization target is not a directory: {}",
                    target.display()
                ),
                false,
                "Choose a nonexistent or empty directory.",
            ));
        }
        if fs::read_dir(&target)
            .map_err(|error| io_error("read", &target, &error))?
            .next()
            .is_some()
        {
            return Err(target_not_empty(&target));
        }
        initialize_existing_empty(&target, vault_id)?
    } else {
        initialize_nonexistent(&target, vault_id)?
    };

    Ok(InitReport {
        vault_id,
        root: target,
        created_files: created_file_names(),
        warnings,
    })
}

fn initialize_nonexistent(target: &Path, vault_id: Uuid) -> Result<Vec<String>, KbError> {
    let parent = target.parent().ok_or_else(|| {
        KbError::new(
            ErrorCode::UnsafePath,
            format!("Initialization target has no parent: {}", target.display()),
            false,
            "Choose a path below an existing directory.",
        )
    })?;
    if !parent.is_dir() {
        return Err(KbError::new(
            ErrorCode::UnsafePath,
            format!("Initialization parent does not exist: {}", parent.display()),
            false,
            "Create the parent directory, then run kb init again.",
        ));
    }
    ensure_not_link_or_reparse_point(parent)?;

    let stage = parent.join(format!(".kb-init-{}", Uuid::new_v4()));
    create_private_directory(&stage)?;
    let populate_result = populate_and_validate(&stage, vault_id);
    if let Err(error) = populate_result {
        return cleanup_after_error(&stage, error);
    }
    if let Err(error) = fs::rename(&stage, target) {
        let primary = io_error("move staged Vault to", target, &error);
        return cleanup_after_error(&stage, primary);
    }
    Ok(Vec::new())
}

fn initialize_existing_empty(target: &Path, vault_id: Uuid) -> Result<Vec<String>, KbError> {
    let stage = target.join(format!(".kb-init-{}", Uuid::new_v4()));
    create_private_directory(&stage)?;
    let populate_result = populate_and_validate(&stage, vault_id);
    if let Err(error) = populate_result {
        return cleanup_after_error(&stage, error);
    }

    let mut moved = Vec::new();
    for name in ["admission.yml", "KB.md", "Wiki", ".kb"] {
        if let Err(error) = fs::rename(stage.join(name), target.join(name)) {
            let primary = io_error("move staged entry into", &target.join(name), &error);
            let mut cleanup_failures = Vec::new();
            for moved_name in moved.iter().rev() {
                let moved_path = target.join(moved_name);
                if let Err(cleanup_error) = remove_owned_path(&moved_path) {
                    cleanup_failures.push(format!("{}: {cleanup_error}", moved_path.display()));
                }
            }
            if let Err(cleanup_error) = remove_owned_path(&stage) {
                cleanup_failures.push(format!("{}: {cleanup_error}", stage.display()));
            }
            if cleanup_failures.is_empty() {
                return Err(primary);
            }
            return Err(cleanup_failure(&stage, &primary, &cleanup_failures));
        }
        moved.push(name);
    }
    match fs::remove_dir(&stage) {
        Ok(()) => Ok(Vec::new()),
        Err(error) => Ok(vec![format!(
            "Vault initialized, but the empty staging directory {} could not be removed: {error}",
            stage.display()
        )]),
    }
}

fn populate_and_validate(stage: &Path, vault_id: Uuid) -> Result<(), KbError> {
    for relative in EMPTY_DIRECTORIES {
        create_private_directory_all(&stage.join(relative))?;
    }
    for (relative, contents) in STATIC_FILES {
        let path = stage.join(relative);
        if let Some(parent) = path.parent() {
            create_private_directory_all(parent)?;
        }
        atomic_replace(&path, contents.as_bytes())?;
    }
    atomic_replace(
        &stage.join(".kb/config.yml"),
        config_yaml(vault_id).as_bytes(),
    )?;
    validate_stage(stage)
}

fn validate_stage(stage: &Path) -> Result<(), KbError> {
    for relative in ["admission.yml", ".kb/config.yml"] {
        let path = stage.join(relative);
        let bytes = fs::read(&path).map_err(|error| io_error("read", &path, &error))?;
        serde_yaml_ng::from_slice::<serde_yaml_ng::Value>(&bytes)
            .map_err(|error| KbError::invalid_config(relative, error.to_string()))?;
    }
    for relative in [
        ".kb/schemas/admission.schema.json",
        ".kb/schemas/config.schema.json",
    ] {
        let path = stage.join(relative);
        let bytes = fs::read(&path).map_err(|error| io_error("read", &path, &error))?;
        serde_json::from_slice::<serde_json::Value>(&bytes)
            .map_err(|error| KbError::invalid_config(relative, error.to_string()))?;
    }
    Ok(())
}

fn absolute_path(path: &Path) -> Result<PathBuf, KbError> {
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }
    std::env::current_dir()
        .map(|current| current.join(path))
        .map_err(|error| io_error("resolve", path, &error))
}

fn create_private_directory(path: &Path) -> Result<(), KbError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;

        let mut builder = fs::DirBuilder::new();
        builder.mode(0o700);
        builder
            .create(path)
            .map_err(|error| io_error("create private directory", path, &error))
    }
    #[cfg(not(unix))]
    fs::create_dir(path).map_err(|error| io_error("create private directory", path, &error))
}

fn create_private_directory_all(path: &Path) -> Result<(), KbError> {
    fs::create_dir_all(path).map_err(|error| io_error("create directory", path, &error))
}

fn cleanup_after_error<T>(owned_path: &Path, primary: KbError) -> Result<T, KbError> {
    match remove_owned_path(owned_path) {
        Ok(()) => Err(primary),
        Err(cleanup) => Err(cleanup_failure(
            owned_path,
            &primary,
            &[cleanup.to_string()],
        )),
    }
}

fn cleanup_failure(owned_path: &Path, primary: &KbError, failures: &[String]) -> KbError {
    KbError::new(
        ErrorCode::VaultNeedsRecovery,
        format!("{primary}; cleanup also failed: {}", failures.join("; ")),
        false,
        format!(
            "Inspect and remove only the reported generated paths under {}.",
            owned_path.display()
        ),
    )
}

fn remove_owned_path(path: &Path) -> Result<(), std::io::Error> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    if metadata.file_type().is_symlink() || metadata.is_file() {
        fs::remove_file(path)
    } else if is_windows_reparse_point(&metadata) {
        fs::remove_dir(path)
    } else {
        fs::remove_dir_all(path)
    }
}

#[cfg(windows)]
fn is_windows_reparse_point(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;

    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
const fn is_windows_reparse_point(_metadata: &fs::Metadata) -> bool {
    false
}

fn target_not_empty(path: &Path) -> KbError {
    KbError::new(
        ErrorCode::TargetNotEmpty,
        format!("Initialization target is not empty: {}", path.display()),
        false,
        "Choose an empty directory or use kb adopt for existing content.",
    )
}

fn io_error(action: &str, path: &Path, error: &std::io::Error) -> KbError {
    KbError::io_failure(action, path.display().to_string(), error.to_string())
}

fn created_file_names() -> Vec<String> {
    STATIC_FILES
        .iter()
        .map(|(path, _)| (*path).to_owned())
        .chain(std::iter::once(".kb/config.yml".to_owned()))
        .collect()
}
