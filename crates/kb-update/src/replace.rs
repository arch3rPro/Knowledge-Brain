use std::{
    fs,
    io::Read,
    path::PathBuf,
    thread,
    time::{Duration, Instant},
};

use sha2::{Digest, Sha256};
use sysinfo::{Pid, ProcessesToUpdate, System};

use crate::UpdateError;

const CLEANUP_MARKER: &str = ".cleanup-token";

/// The three sibling paths used for one recoverable executable replacement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplaceRequest {
    pub from: PathBuf,
    pub to: PathBuf,
    pub backup: PathBuf,
    pub expected_sha256: Option<String>,
}

impl ReplaceRequest {
    /// Creates a replacement request from explicit paths.
    #[must_use]
    pub fn new(
        from: impl Into<PathBuf>,
        to: impl Into<PathBuf>,
        backup: impl Into<PathBuf>,
    ) -> Self {
        Self {
            from: from.into(),
            to: to.into(),
            backup: backup.into(),
            expected_sha256: None,
        }
    }

    /// Creates a replacement request that verifies the installed executable.
    #[must_use]
    pub fn verified(
        from: impl Into<PathBuf>,
        to: impl Into<PathBuf>,
        backup: impl Into<PathBuf>,
        expected_sha256: impl Into<String>,
    ) -> Self {
        Self {
            from: from.into(),
            to: to.into(),
            backup: backup.into(),
            expected_sha256: Some(expected_sha256.into()),
        }
    }
}

/// Waits until the process identified by `parent_pid` no longer exists.
///
/// # Errors
///
/// Returns [`UpdateError::ReplacementFailed`] for an invalid process ID or if
/// the parent does not exit before the bounded helper wait elapses.
pub fn parent_process_start_time(parent_pid: u32) -> Result<u64, UpdateError> {
    if parent_pid == 0 {
        return Err(UpdateError::ReplacementFailed(
            "parent process ID must be non-zero".into(),
        ));
    }
    let parent = Pid::from_u32(parent_pid);
    let mut system = System::new();
    system.refresh_processes(ProcessesToUpdate::Some(&[parent]), true);
    system
        .process(parent)
        .map(sysinfo::Process::start_time)
        .ok_or_else(|| UpdateError::ReplacementFailed("parent process does not exist".into()))
}

/// Waits for one exact process identity, not merely a reusable process ID.
///
/// # Errors
///
/// Returns [`UpdateError::ReplacementFailed`] when the identity differs or the
/// bounded wait elapses.
pub fn wait_for_parent_exit(parent_pid: u32, parent_start_time: u64) -> Result<(), UpdateError> {
    if parent_pid == 0 {
        return Err(UpdateError::ReplacementFailed(
            "parent process ID must be non-zero".into(),
        ));
    }
    let parent = Pid::from_u32(parent_pid);
    let deadline = Instant::now() + Duration::from_secs(300);
    let mut system = System::new();
    loop {
        system.refresh_processes(ProcessesToUpdate::Some(&[parent]), true);
        match system.process(parent) {
            None => return Ok(()),
            Some(process) if process.start_time() != parent_start_time => {
                return Err(UpdateError::ReplacementFailed(
                    "parent process identity no longer matches".into(),
                ));
            }
            Some(_) => {}
        }
        if Instant::now() >= deadline {
            return Err(UpdateError::ReplacementFailed(
                "timed out waiting for the parent process to exit".into(),
            ));
        }
        thread::sleep(Duration::from_millis(50));
    }
}

/// Replaces `to` with `from`, restoring `to` if the new file cannot be moved.
///
/// # Errors
///
/// Returns [`UpdateError::ReplacementFailed`] if paths are unsafe to replace or
/// if the old executable cannot be restored after a failed replacement.
pub fn replace_with_backup(request: &ReplaceRequest) -> Result<(), UpdateError> {
    if request.backup.exists() {
        return Err(UpdateError::ReplacementFailed(
            "refusing to overwrite an existing backup".into(),
        ));
    }
    rename_with_retry(&request.to, &request.backup).map_err(|error| {
        UpdateError::ReplacementFailed(format!("move current executable: {error}"))
    })?;
    if let Err(error) = rename_with_retry(&request.from, &request.to) {
        return restore_backup(request, format!("replace executable: {error}"));
    }
    if let Some(expected) = &request.expected_sha256 {
        let actual = match sha256_file(&request.to) {
            Ok(actual) => actual,
            Err(error) => {
                let _ = fs::remove_file(&request.to);
                return restore_backup(request, error.to_string());
            }
        };
        if &actual != expected {
            fs::remove_file(&request.to).map_err(|error| {
                UpdateError::ReplacementFailed(format!(
                    "new executable checksum mismatch; remove invalid executable: {error}; backup remains at {}",
                    request.backup.display()
                ))
            })?;
            return restore_backup(request, "new executable checksum mismatch".into());
        }
    }
    fs::remove_file(&request.backup)
        .map_err(|error| UpdateError::ReplacementFailed(format!("remove backup: {error}")))
}

/// Removes a private updater stage only when its name and marker match.
///
/// # Errors
///
/// Returns [`UpdateError::ReplacementFailed`] without deleting anything when
/// the directory is not a regular updater stage or the token differs.
pub fn remove_update_stage(directory: &std::path::Path, token: &str) -> Result<(), UpdateError> {
    let name = directory.file_name().and_then(|name| name.to_str());
    let metadata = fs::symlink_metadata(directory).map_err(|error| {
        UpdateError::ReplacementFailed(format!("inspect update stage: {error}"))
    })?;
    if !metadata.is_dir() || name.is_none_or(|name| !name.starts_with(".kb-update-")) {
        return Err(UpdateError::ReplacementFailed(
            "refusing to remove a directory that is not an update stage".into(),
        ));
    }
    let marker = directory.join(CLEANUP_MARKER);
    let marker_metadata = fs::symlink_metadata(&marker).map_err(|error| {
        UpdateError::ReplacementFailed(format!("inspect update stage marker: {error}"))
    })?;
    if !marker_metadata.is_file() {
        return Err(UpdateError::ReplacementFailed(
            "update stage marker is not a regular file".into(),
        ));
    }
    let actual = fs::read_to_string(&marker).map_err(|error| {
        UpdateError::ReplacementFailed(format!("read update stage marker: {error}"))
    })?;
    if token.is_empty() || actual != token {
        return Err(UpdateError::ReplacementFailed(
            "update stage marker does not match".into(),
        ));
    }
    fs::remove_dir_all(directory)
        .map_err(|error| UpdateError::ReplacementFailed(format!("remove update stage: {error}")))
}

fn restore_backup(request: &ReplaceRequest, reason: String) -> Result<(), UpdateError> {
    match rename_with_retry(&request.backup, &request.to) {
        Ok(()) => Err(UpdateError::ReplacementFailed(reason)),
        Err(restore_error) => Err(UpdateError::ReplacementFailed(format!(
            "{reason}; restore previous executable: {restore_error}; backup remains at {}",
            request.backup.display()
        ))),
    }
}

#[cfg(not(windows))]
fn rename_with_retry(from: &std::path::Path, to: &std::path::Path) -> std::io::Result<()> {
    fs::rename(from, to)
}

#[cfg(windows)]
fn rename_with_retry(from: &std::path::Path, to: &std::path::Path) -> std::io::Result<()> {
    const ATTEMPTS: usize = 20;
    let mut last_error = None;
    for attempt in 0..ATTEMPTS {
        match fs::rename(from, to) {
            Ok(()) => return Ok(()),
            Err(error)
                if error.kind() == std::io::ErrorKind::PermissionDenied
                    && attempt + 1 < ATTEMPTS =>
            {
                last_error = Some(error);
                thread::sleep(Duration::from_millis(100));
            }
            Err(error) => return Err(error),
        }
    }
    Err(last_error.expect("at least one rename attempt"))
}

fn sha256_file(path: &std::path::Path) -> Result<String, UpdateError> {
    let mut input = fs::File::open(path).map_err(|error| {
        UpdateError::ReplacementFailed(format!("open installed executable: {error}"))
    })?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 64 * 1024].into_boxed_slice();
    loop {
        let count = input.read(&mut buffer).map_err(|error| {
            UpdateError::ReplacementFailed(format!("read installed executable: {error}"))
        })?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(hex::encode(hasher.finalize()))
}
