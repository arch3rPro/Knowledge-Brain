use std::{fs, path::PathBuf};

use kb_core::{AdoptionPlan, ErrorCode, KbError, OperationId};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use uuid::Uuid;

use crate::{UserPaths, atomic_replace};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdoptionResult {
    pub operation_id: OperationId,
    pub vault_id: Uuid,
    pub target: PathBuf,
    pub created: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OperationState {
    Planned(AdoptionPlan),
    Applied(AdoptionResult),
}

/// Read an operation plan or its durable completion receipt.
///
/// # Errors
///
/// Returns [`KbError`] when the operation is unknown or its JSON is invalid.
pub fn inspect_operation(
    user_paths: &UserPaths,
    operation_id: OperationId,
) -> Result<OperationState, KbError> {
    let directory = operation_directory(user_paths, operation_id);
    let result_path = directory.join("result.json");
    if result_path.is_file() {
        return read_json(&result_path).map(OperationState::Applied);
    }
    let plan_path = directory.join("plan.json");
    if plan_path.is_file() {
        return read_json(&plan_path).map(OperationState::Planned);
    }
    Err(KbError::new(
        ErrorCode::OperationNotFound,
        format!("Operation {operation_id} does not exist."),
        false,
        "Create a new plan with kb adopt <directory>.",
    ))
}

pub(crate) fn operation_directory(user_paths: &UserPaths, operation_id: OperationId) -> PathBuf {
    user_paths
        .state_dir
        .join("operations")
        .join(operation_id.to_string())
}

pub(crate) fn save_plan(user_paths: &UserPaths, plan: &AdoptionPlan) -> Result<(), KbError> {
    let directory = operation_directory(user_paths, plan.operation_id);
    create_private_directory_all(&directory)?;
    write_json(&directory.join("plan.json"), plan)
}

pub(crate) fn save_result(user_paths: &UserPaths, result: &AdoptionResult) -> Result<(), KbError> {
    let directory = operation_directory(user_paths, result.operation_id);
    create_private_directory_all(&directory)?;
    write_json(&directory.join("result.json"), result)?;
    let plan_path = directory.join("plan.json");
    match fs::remove_file(&plan_path) {
        Ok(()) => remove_progress_file(&directory),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            remove_progress_file(&directory)
        }
        Err(error) => Err(KbError::io_failure(
            "remove completed plan",
            plan_path.display().to_string(),
            error.to_string(),
        )),
    }
}

fn remove_progress_file(directory: &std::path::Path) -> Result<(), KbError> {
    let path = directory.join("progress.json");
    match fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(KbError::io_failure(
            "remove completed progress",
            path.display().to_string(),
            error.to_string(),
        )),
    }
}

pub(crate) fn read_json<T: DeserializeOwned>(path: &std::path::Path) -> Result<T, KbError> {
    let bytes = fs::read(path).map_err(|error| {
        KbError::io_failure("read", path.display().to_string(), error.to_string())
    })?;
    serde_json::from_slice(&bytes)
        .map_err(|error| KbError::invalid_config(path.display().to_string(), error.to_string()))
}

pub(crate) fn write_json(path: &std::path::Path, value: &impl Serialize) -> Result<(), KbError> {
    let bytes = serde_json::to_vec_pretty(value)
        .map_err(|error| KbError::invalid_config(path.display().to_string(), error.to_string()))?;
    atomic_replace(path, &bytes)
}

fn create_private_directory_all(path: &std::path::Path) -> Result<(), KbError> {
    fs::create_dir_all(path).map_err(|error| {
        KbError::io_failure(
            "create private directory",
            path.display().to_string(),
            error.to_string(),
        )
    })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(|error| {
            KbError::io_failure(
                "set private permissions on",
                path.display().to_string(),
                error.to_string(),
            )
        })?;
    }
    Ok(())
}
