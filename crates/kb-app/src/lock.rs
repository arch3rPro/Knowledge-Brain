use std::{fs, fs::File, path::Path, path::PathBuf, time::SystemTime};

use kb_core::{ErrorCode, KbError, OperationId};
use serde::Serialize;

use crate::atomic_replace;

#[derive(Debug, Clone, Copy)]
pub enum LockMode {
    Shared,
    Exclusive,
}

pub struct VaultLock {
    file: File,
    info_path: Option<PathBuf>,
}

#[derive(Serialize)]
struct LockInfo<'a> {
    command: &'a str,
    process_id: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    operation_id: Option<OperationId>,
    started_at_unix_ms: u128,
}

impl VaultLock {
    /// Acquire a nonblocking operating-system lock for one Vault.
    ///
    /// # Errors
    ///
    /// Returns [`KbError`] if the lock cannot be opened or is already held in
    /// an incompatible mode.
    pub fn acquire(
        root: &Path,
        mode: LockMode,
        command: &str,
        operation_id: Option<OperationId>,
    ) -> Result<Self, KbError> {
        let runtime = root.join(".kb/runtime");
        fs::create_dir_all(&runtime)
            .map_err(|error| io_error("create runtime directory", &runtime, &error))?;
        let lock_path = runtime.join("vault.lock");
        let file = File::options()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)
            .map_err(|error| io_error("open Vault lock", &lock_path, &error))?;
        let acquired = match mode {
            LockMode::Shared => fs2::FileExt::try_lock_shared(&file),
            LockMode::Exclusive => fs2::FileExt::try_lock_exclusive(&file),
        };
        if let Err(error) = acquired {
            return Err(lock_busy(root, &error));
        }

        let info_path = if matches!(mode, LockMode::Exclusive) {
            let path = runtime.join("vault-lock-info.json");
            let info = LockInfo {
                command,
                process_id: std::process::id(),
                operation_id,
                started_at_unix_ms: SystemTime::now()
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .map_or(0, |duration| duration.as_millis()),
            };
            let bytes = serde_json::to_vec_pretty(&info).map_err(|serialization_error| {
                KbError::invalid_config("Vault lock information", serialization_error.to_string())
            })?;
            atomic_replace(&path, &bytes)?;
            Some(path)
        } else {
            None
        };
        Ok(Self { file, info_path })
    }
}

impl Drop for VaultLock {
    fn drop(&mut self) {
        if let Some(path) = &self.info_path {
            let _ = fs::remove_file(path);
        }
        let _ = fs2::FileExt::unlock(&self.file);
    }
}

pub(crate) fn probe_exclusive_lock(root: &Path) -> Result<(), KbError> {
    let lock_path = root.join(".kb/runtime/vault.lock");
    let existed = lock_path.exists();
    {
        let _guard = VaultLock::acquire(root, LockMode::Exclusive, "doctor", None)?;
    }
    if !existed {
        match fs::remove_file(&lock_path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(io_error("remove doctor lock file", &lock_path, &error)),
        }
    }
    Ok(())
}

fn lock_busy(root: &Path, error: &std::io::Error) -> KbError {
    let info_path = root.join(".kb/runtime/vault-lock-info.json");
    let details = fs::read_to_string(&info_path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok());
    KbError::new(
        ErrorCode::WriteBusy,
        format!("Vault is busy: {error}"),
        true,
        "Wait for the reported command to finish, then retry.",
    )
    .with_details(details.unwrap_or_else(|| serde_json::json!({ "lock_info": "unavailable" })))
}

fn io_error(action: &str, path: &Path, error: &std::io::Error) -> KbError {
    KbError::io_failure(action, path.display().to_string(), error.to_string())
}
