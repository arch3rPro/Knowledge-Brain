use std::fs;
use std::path::{Path, PathBuf};

use kb_core::{
    ErrorCode, KbError, OperationId, UpdateComponent, UpdateConfirmationToken,
    UpdateExecutionState, UpdateOperation, UpdatePhase, UpdatePlan,
    ensure_not_link_or_reparse_point,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

use crate::operation::{create_private_directory_all, read_json, write_json};
use crate::{UserPaths, atomic_replace};

#[derive(Debug, Clone)]
pub struct UpdateStore {
    root: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredOperation {
    #[serde(flatten)]
    operation: UpdateOperation,
    plan_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct UpdateEffects {
    kind: String,
    operation_id: OperationId,
    components: Vec<UpdateComponent>,
}

impl UpdateStore {
    #[must_use]
    pub fn new(user_paths: &UserPaths) -> Self {
        Self {
            root: user_paths.state_dir.join("updates"),
        }
    }

    /// Persist an immutable preview and make it the latest update operation.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid plans, unsafe paths, duplicate operation IDs,
    /// or durable-write failures.
    pub fn create(&self, plan: &UpdatePlan) -> Result<UpdateOperation, KbError> {
        validate_new_plan(plan)?;
        self.ensure_root()?;
        let directory = self.operation_directory(plan.operation_id);
        create_new_private_directory(&directory)?;

        let plan_bytes = serde_json::to_vec_pretty(plan).map_err(|error| {
            KbError::invalid_config(
                directory.join("plan.json").display().to_string(),
                error.to_string(),
            )
        })?;
        let plan_sha256 = digest(&plan_bytes);
        let now = now_text()?;
        let operation = UpdateOperation {
            kind: "update".into(),
            phase: UpdatePhase::Preview,
            operation_id: plan.operation_id,
            current_version: plan.current_version.clone(),
            target_version: plan.target_version.clone(),
            scope: plan.scope.clone(),
            components: plan.components.clone(),
            conflicts: plan.conflicts.clone(),
            skipped: plan.skipped.clone(),
            untouched: plan.untouched.clone(),
            execution_state: UpdateExecutionState::Preview,
            created_at: plan.created_at.clone(),
            updated_at: now,
            completed_at: None,
        };
        let stored = StoredOperation {
            operation: operation.clone(),
            plan_sha256,
        };
        let effects = UpdateEffects {
            kind: "update_effects".into(),
            operation_id: plan.operation_id,
            components: Vec::new(),
        };

        atomic_replace(&directory.join("plan.json"), &plan_bytes)?;
        write_json(&directory.join("operation.json"), &stored)?;
        write_json(&directory.join("effects.json"), &effects)?;
        atomic_replace(
            &self.root.join("latest"),
            plan.operation_id.to_string().as_bytes(),
        )?;
        Ok(operation)
    }

    /// Load a durable update operation, including terminal receipts.
    ///
    /// # Errors
    ///
    /// Returns an error when the operation is absent, unsafe, or malformed.
    pub fn load(&self, operation_id: OperationId) -> Result<UpdateOperation, KbError> {
        let directory = self.checked_operation_directory(operation_id)?;
        let stored: StoredOperation = read_json(&directory.join("operation.json"))?;
        validate_stored_identity(&stored, operation_id)?;
        Ok(stored.operation)
    }

    /// Confirm exactly the immutable preview represented by token.
    ///
    /// # Errors
    ///
    /// Returns an error when the token, plan bytes, expiry, identity, or state
    /// does not match the persisted preview.
    pub fn confirm(
        &self,
        operation_id: OperationId,
        token: &UpdateConfirmationToken,
    ) -> Result<UpdateOperation, KbError> {
        let directory = self.checked_operation_directory(operation_id)?;
        let operation_path = directory.join("operation.json");
        let mut stored: StoredOperation = read_json(&operation_path)?;
        validate_stored_identity(&stored, operation_id)?;
        if stored.operation.execution_state != UpdateExecutionState::Preview
            || stored.operation.phase != UpdatePhase::Preview
        {
            return Err(invalid_transition(
                stored.operation.execution_state,
                UpdateExecutionState::Confirmed,
            ));
        }

        let plan_path = directory.join("plan.json");
        ensure_regular_file(&plan_path)?;
        let plan_bytes = fs::read(&plan_path)
            .map_err(|error| io_error("read immutable update plan", &plan_path, &error))?;
        if digest(&plan_bytes) != stored.plan_sha256 {
            return Err(stale_plan(
                "The persisted update plan changed after it was created.",
            ));
        }
        let plan: UpdatePlan = serde_json::from_slice(&plan_bytes).map_err(|error| {
            KbError::invalid_config(plan_path.display().to_string(), error.to_string())
        })?;
        if plan.operation_id != operation_id || plan.phase != UpdatePhase::Preview {
            return Err(stale_plan("The persisted update plan identity is invalid."));
        }
        if !operation_matches_plan(&stored.operation, &plan) {
            return Err(stale_plan(
                "The persisted update operation differs from its immutable plan.",
            ));
        }
        let expected = plan.confirmation_token()?;
        if &expected != token {
            return Err(stale_plan(
                "The confirmation token does not match the persisted update plan.",
            ));
        }
        let expires_at = parse_time(&plan.expires_at, "update plan expiry")?;
        if expires_at <= OffsetDateTime::now_utc() {
            return Err(stale_plan(
                "The update plan expired; create and review a new plan.",
            ));
        }

        set_state(&mut stored.operation, UpdateExecutionState::Confirmed)?;
        write_json(&operation_path, &stored)?;
        Ok(stored.operation)
    }

    /// Move an operation through one legal execution state transition.
    ///
    /// # Errors
    ///
    /// Returns an error when the operation is unsafe, malformed, or the
    /// requested transition is not part of the update state machine.
    pub fn transition(
        &self,
        operation_id: OperationId,
        next: UpdateExecutionState,
    ) -> Result<UpdateOperation, KbError> {
        let directory = self.checked_operation_directory(operation_id)?;
        let path = directory.join("operation.json");
        let mut stored: StoredOperation = read_json(&path)?;
        validate_stored_identity(&stored, operation_id)?;
        if !legal_transition(stored.operation.execution_state, next) {
            return Err(invalid_transition(stored.operation.execution_state, next));
        }
        set_state(&mut stored.operation, next)?;
        write_json(&path, &stored)?;
        Ok(stored.operation)
    }

    /// Record one component result in both the effect receipt and operation.
    ///
    /// # Errors
    ///
    /// Returns an error unless the operation is applying components and the
    /// component belongs to the confirmed plan.
    pub fn record_component(
        &self,
        operation_id: OperationId,
        component: UpdateComponent,
    ) -> Result<UpdateOperation, KbError> {
        let directory = self.checked_operation_directory(operation_id)?;
        let operation_path = directory.join("operation.json");
        let mut stored: StoredOperation = read_json(&operation_path)?;
        validate_stored_identity(&stored, operation_id)?;
        if stored.operation.execution_state != UpdateExecutionState::ApplyingComponents
            || component.state == kb_core::UpdateComponentState::Pending
        {
            return Err(KbError::new(
                ErrorCode::InvalidConfig,
                "Cannot record this update component result.",
                false,
                "Resume the confirmed update before recording component results.",
            ));
        }
        let Some(slot) = stored
            .operation
            .components
            .iter_mut()
            .find(|candidate| candidate.id == component.id)
        else {
            return Err(KbError::invalid_config(
                "update component",
                format!("{} is not part of the persisted plan", component.id),
            ));
        };
        if slot.kind != component.kind || slot.vault_id != component.vault_id {
            return Err(stale_plan(
                "The component identity differs from the persisted update plan.",
            ));
        }
        *slot = component.clone();

        let effects_path = directory.join("effects.json");
        let mut effects: UpdateEffects = read_json(&effects_path)?;
        if effects.kind != "update_effects" || effects.operation_id != operation_id {
            return Err(KbError::invalid_config(
                effects_path.display().to_string(),
                "effect receipt identity is invalid",
            ));
        }
        match effects
            .components
            .iter_mut()
            .find(|candidate| candidate.id == component.id)
        {
            Some(existing) => *existing = component,
            None => effects.components.push(component),
        }
        effects
            .components
            .sort_by(|left, right| left.id.cmp(&right.id));

        // Persist the effect first so an interrupted operation can recover it.
        write_json(&effects_path, &effects)?;
        stored.operation.updated_at = now_text()?;
        write_json(&operation_path, &stored)?;
        Ok(stored.operation)
    }

    /// Load the operation referenced by the atomic latest pointer.
    ///
    /// # Errors
    ///
    /// Returns an error when the pointer or referenced operation is malformed.
    pub fn latest(&self) -> Result<Option<UpdateOperation>, KbError> {
        match fs::symlink_metadata(self.root.join("latest")) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() || !metadata.is_file() {
                    return Err(unsafe_path(&self.root.join("latest")));
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => {
                return Err(io_error(
                    "inspect latest update pointer",
                    &self.root,
                    &error,
                ));
            }
        }
        let path = self.root.join("latest");
        let value = fs::read_to_string(&path)
            .map_err(|error| io_error("read latest update pointer", &path, &error))?;
        if value.trim().is_empty() {
            return Ok(None);
        }
        let operation_id = value.trim().parse::<OperationId>().map_err(|error| {
            KbError::invalid_config(path.display().to_string(), error.to_string())
        })?;
        self.load(operation_id).map(Some)
    }

    /// Remove a never-confirmed preview and its generated staging files.
    ///
    /// # Errors
    ///
    /// Returns an error for confirmed or terminal operations and unsafe paths.
    pub fn remove_cancelled(&self, operation_id: OperationId) -> Result<(), KbError> {
        let operation = self.load(operation_id)?;
        if operation.execution_state != UpdateExecutionState::Preview {
            return Err(KbError::new(
                ErrorCode::InvalidConfig,
                "Only an unconfirmed update preview can be cancelled.",
                false,
                "Keep this operation receipt and use update status or resume.",
            ));
        }
        let directory = self.checked_operation_directory(operation_id)?;
        remove_tree_without_links(&directory)?;

        let latest_path = self.root.join("latest");
        if fs::read_to_string(&latest_path)
            .is_ok_and(|value| value.trim() == operation_id.to_string())
        {
            atomic_replace(&latest_path, b"")?;
        }
        Ok(())
    }

    fn ensure_root(&self) -> Result<(), KbError> {
        let state_dir = self.root.parent().ok_or_else(|| {
            KbError::invalid_config("update state directory", "missing parent directory")
        })?;
        match fs::symlink_metadata(state_dir) {
            Ok(metadata) if metadata.is_dir() => ensure_not_link_or_reparse_point(state_dir)?,
            Ok(_) => return Err(unsafe_path(state_dir)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                create_private_directory_all(state_dir)?;
                ensure_not_link_or_reparse_point(state_dir)?;
            }
            Err(error) => {
                return Err(io_error(
                    "inspect update state parent directory",
                    state_dir,
                    &error,
                ));
            }
        }
        match fs::symlink_metadata(&self.root) {
            Ok(metadata) if metadata.is_dir() => ensure_not_link_or_reparse_point(&self.root),
            Ok(_) => Err(unsafe_path(&self.root)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                create_new_private_directory(&self.root)
            }
            Err(error) => Err(io_error(
                "inspect update state directory",
                &self.root,
                &error,
            )),
        }
    }

    fn checked_operation_directory(&self, operation_id: OperationId) -> Result<PathBuf, KbError> {
        ensure_not_link_or_reparse_point(&self.root)?;
        let directory = self.operation_directory(operation_id);
        match fs::symlink_metadata(&directory) {
            Ok(metadata) if metadata.is_dir() => {
                ensure_not_link_or_reparse_point(&directory)?;
                Ok(directory)
            }
            Ok(_) => Err(unsafe_path(&directory)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Err(KbError::new(
                ErrorCode::OperationNotFound,
                format!("Update operation {operation_id} does not exist."),
                false,
                "Run kb update to create a new update plan.",
            )),
            Err(error) => Err(io_error("inspect update operation", &directory, &error)),
        }
    }

    fn operation_directory(&self, operation_id: OperationId) -> PathBuf {
        self.root.join(operation_id.to_string())
    }
}

fn operation_matches_plan(operation: &UpdateOperation, plan: &UpdatePlan) -> bool {
    operation.operation_id == plan.operation_id
        && operation.current_version == plan.current_version
        && operation.target_version == plan.target_version
        && operation.scope == plan.scope
        && operation.components == plan.components
        && operation.conflicts == plan.conflicts
        && operation.skipped == plan.skipped
        && operation.untouched == plan.untouched
        && operation.created_at == plan.created_at
}

fn validate_new_plan(plan: &UpdatePlan) -> Result<(), KbError> {
    plan.validate()?;
    if plan.phase != UpdatePhase::Preview {
        return Err(KbError::invalid_config(
            "update plan",
            "new plans must be in preview phase",
        ));
    }
    let created_at = parse_time(&plan.created_at, "update plan creation time")?;
    let expires_at = parse_time(&plan.expires_at, "update plan expiry")?;
    if expires_at <= created_at {
        return Err(KbError::invalid_config(
            "update plan",
            "expiry must be later than creation time",
        ));
    }
    Ok(())
}

fn validate_stored_identity(
    stored: &StoredOperation,
    expected: OperationId,
) -> Result<(), KbError> {
    if stored.operation.kind != "update"
        || stored.operation.operation_id != expected
        || stored.plan_sha256.len() != 64
        || !stored
            .plan_sha256
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(KbError::invalid_config(
            "update operation",
            "operation identity or plan digest is invalid",
        ));
    }
    Ok(())
}

fn legal_transition(current: UpdateExecutionState, next: UpdateExecutionState) -> bool {
    matches!(
        (current, next),
        (
            UpdateExecutionState::Confirmed,
            UpdateExecutionState::ReplacingCli
                | UpdateExecutionState::ApplyingComponents
                | UpdateExecutionState::Failed
        ) | (
            UpdateExecutionState::ReplacingCli,
            UpdateExecutionState::CliReplaced | UpdateExecutionState::Failed
        ) | (
            UpdateExecutionState::CliReplaced,
            UpdateExecutionState::ApplyingComponents | UpdateExecutionState::Failed
        ) | (
            UpdateExecutionState::ApplyingComponents,
            UpdateExecutionState::Completed
                | UpdateExecutionState::CompletedWithSkips
                | UpdateExecutionState::Partial
                | UpdateExecutionState::Failed
        )
    )
}

fn set_state(operation: &mut UpdateOperation, state: UpdateExecutionState) -> Result<(), KbError> {
    operation.execution_state = state;
    operation.phase = match state {
        UpdateExecutionState::Preview => UpdatePhase::Preview,
        UpdateExecutionState::Confirmed => UpdatePhase::Confirmed,
        UpdateExecutionState::ReplacingCli
        | UpdateExecutionState::CliReplaced
        | UpdateExecutionState::ApplyingComponents => UpdatePhase::Running,
        UpdateExecutionState::Completed
        | UpdateExecutionState::CompletedWithSkips
        | UpdateExecutionState::Partial
        | UpdateExecutionState::Failed => UpdatePhase::Applied,
    };
    let now = now_text()?;
    operation.updated_at.clone_from(&now);
    operation.completed_at = state.is_terminal().then_some(now);
    Ok(())
}

fn create_new_private_directory(path: &Path) -> Result<(), KbError> {
    match fs::symlink_metadata(path) {
        Ok(_) => {
            return Err(KbError::new(
                ErrorCode::InvalidConfig,
                format!("Update state path already exists: {}", path.display()),
                false,
                "Inspect the existing update operation before retrying.",
            ));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(io_error("inspect update state path", path, &error)),
    }
    fs::create_dir(path)
        .map_err(|error| io_error("create private update directory", path, &error))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .map_err(|error| io_error("set private update permissions", path, &error))?;
    }
    Ok(())
}

fn ensure_regular_file(path: &Path) -> Result<(), KbError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => Ok(()),
        Ok(_) => Err(unsafe_path(path)),
        Err(error) => Err(io_error("inspect update state file", path, &error)),
    }
}

fn remove_tree_without_links(path: &Path) -> Result<(), KbError> {
    ensure_not_link_or_reparse_point(path)?;
    for entry in
        fs::read_dir(path).map_err(|error| io_error("read update state directory", path, &error))?
    {
        let entry = entry.map_err(|error| io_error("read update state entry", path, &error))?;
        let child = entry.path();
        ensure_not_link_or_reparse_point(&child)?;
        if entry
            .file_type()
            .map_err(|error| io_error("inspect update state entry", &child, &error))?
            .is_dir()
        {
            remove_tree_without_links(&child)?;
        } else {
            fs::remove_file(&child)
                .map_err(|error| io_error("remove cancelled update file", &child, &error))?;
        }
    }
    fs::remove_dir(path)
        .map_err(|error| io_error("remove cancelled update directory", path, &error))
}

fn parse_time(value: &str, field: &str) -> Result<OffsetDateTime, KbError> {
    OffsetDateTime::parse(value, &Rfc3339)
        .map_err(|error| KbError::invalid_config(field, error.to_string()))
}

fn now_text() -> Result<String, KbError> {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .map_err(|error| KbError::invalid_config("update timestamp", error.to_string()))
}

fn digest(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn stale_plan(message: &str) -> KbError {
    KbError::new(
        ErrorCode::PlanStale,
        message,
        false,
        "Run kb update again and review the newly generated plan.",
    )
}

fn invalid_transition(current: UpdateExecutionState, next: UpdateExecutionState) -> KbError {
    KbError::new(
        ErrorCode::InvalidConfig,
        format!("Invalid update state transition from {current:?} to {next:?}."),
        false,
        "Inspect the durable update status before retrying.",
    )
}

fn unsafe_path(path: &Path) -> KbError {
    KbError::new(
        ErrorCode::UnsafePath,
        format!("Unsafe update state path: {}", path.display()),
        false,
        "Replace linked or non-file update state entries with regular private paths.",
    )
}

fn io_error(action: &str, path: &Path, error: &std::io::Error) -> KbError {
    KbError::io_failure(action, path.display().to_string(), error.to_string())
}
