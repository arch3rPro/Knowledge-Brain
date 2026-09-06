use std::{collections::BTreeMap, path::PathBuf};

use directories::ProjectDirs;
use kb_core::{ErrorCode, KbError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserPaths {
    pub config_dir: PathBuf,
    pub state_dir: PathBuf,
    pub cache_dir: PathBuf,
}

impl UserPaths {
    #[must_use]
    pub const fn new(config_dir: PathBuf, state_dir: PathBuf, cache_dir: PathBuf) -> Self {
        Self {
            config_dir,
            state_dir,
            cache_dir,
        }
    }

    /// Resolve user-level paths from explicit environment values and OS defaults.
    ///
    /// # Errors
    ///
    /// Returns [`KbError`] if the operating system has no usable application
    /// directories or an explicit override is empty.
    pub fn resolve(environment: &BTreeMap<String, String>) -> Result<Self, KbError> {
        let project =
            ProjectDirs::from("org", "Knowledge-Brain", "Knowledge-Brain").ok_or_else(|| {
                KbError::new(
                    ErrorCode::InvalidConfig,
                    "Cannot determine operating-system application directories.",
                    false,
                    "Set KB_CONFIG_DIR, KB_STATE_DIR, and KB_CACHE_DIR explicitly.",
                )
            })?;

        Ok(Self::new(
            overridden_path(environment, "KB_CONFIG_DIR")?
                .unwrap_or_else(|| project.config_dir().to_path_buf()),
            overridden_path(environment, "KB_STATE_DIR")?
                .unwrap_or_else(|| project.data_local_dir().join("state")),
            overridden_path(environment, "KB_CACHE_DIR")?
                .unwrap_or_else(|| project.cache_dir().to_path_buf()),
        ))
    }
}

fn overridden_path(
    environment: &BTreeMap<String, String>,
    key: &str,
) -> Result<Option<PathBuf>, KbError> {
    let Some(value) = environment.get(key) else {
        return Ok(None);
    };
    if value.trim().is_empty() {
        return Err(KbError::invalid_config(
            key,
            "path override cannot be empty",
        ));
    }
    let path = PathBuf::from(value);
    if !path.is_absolute() {
        return Err(KbError::invalid_config(
            key,
            "path override must be absolute",
        ));
    }
    Ok(Some(path))
}
