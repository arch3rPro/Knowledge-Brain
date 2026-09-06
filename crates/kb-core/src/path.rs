use std::{
    collections::BTreeMap,
    path::{Component, Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use unicode_normalization::UnicodeNormalization;

use crate::{ErrorCode, KbError};

const WINDOWS_RESERVED_BASENAMES: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PortableRelativePath(String);

impl PortableRelativePath {
    /// Parse a `/`-separated path that is portable across supported systems.
    ///
    /// # Errors
    ///
    /// Returns [`KbError`] when the value is absolute, malformed, or contains a
    /// component forbidden on Windows, macOS, or Linux.
    pub fn parse(value: &str) -> Result<Self, KbError> {
        validate_portable_text(value)?;
        Ok(Self(value.to_owned()))
    }

    /// Convert a native relative path into its portable representation.
    ///
    /// # Errors
    ///
    /// Returns [`KbError`] when the path is not relative and normalized, is not
    /// valid UTF-8, or violates a portable component rule.
    pub fn from_path(path: &Path) -> Result<Self, KbError> {
        let parts = path
            .components()
            .map(|component| match component {
                Component::Normal(value) => value
                    .to_str()
                    .map(ToOwned::to_owned)
                    .ok_or_else(|| unsafe_path(path, "path is not valid UTF-8")),
                _ => Err(unsafe_path(path, "path must be relative and normalized")),
            })
            .collect::<Result<Vec<_>, _>>()?;
        Self::parse(&parts.join("/"))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    #[must_use]
    pub fn to_native_path(&self) -> PathBuf {
        self.0.split('/').collect()
    }
}

/// Validate a framework-generated relative path.
///
/// # Errors
///
/// Returns [`KbError`] when any component is unsafe or non-portable.
pub fn validate_generated_path(path: &Path) -> Result<PortableRelativePath, KbError> {
    PortableRelativePath::from_path(path)
}

/// Validate a single top-level directory from `admission.yml`.
///
/// # Errors
///
/// Returns [`KbError`] when the value is nested, non-portable, or names a
/// framework-owned directory.
pub fn validate_admission_directory(path: &Path) -> Result<PortableRelativePath, KbError> {
    let portable = PortableRelativePath::from_path(path)?;
    if portable.as_str().contains('/') {
        return Err(unsafe_path(
            path,
            "admission path must contain one component",
        ));
    }
    if portable.as_str().eq_ignore_ascii_case("Wiki")
        || portable.as_str().eq_ignore_ascii_case(".kb")
    {
        return Err(unsafe_path(
            path,
            "framework-owned directory is not admissible",
        ));
    }
    Ok(portable)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortabilityCollision {
    pub key: String,
    pub paths: Vec<PathBuf>,
}

/// Find paths that are equivalent under portable case and Unicode rules.
///
/// # Errors
///
/// Returns [`KbError`] when an input path itself is not portable.
pub fn detect_portability_collisions(
    paths: &[PathBuf],
) -> Result<Vec<PortabilityCollision>, KbError> {
    let mut by_key: BTreeMap<String, Vec<PathBuf>> = BTreeMap::new();
    for path in paths {
        let portable = PortableRelativePath::from_path(path)?;
        by_key
            .entry(portability_key(&portable))
            .or_default()
            .push(path.clone());
    }
    Ok(by_key
        .into_iter()
        .filter_map(|(key, paths)| (paths.len() > 1).then_some(PortabilityCollision { key, paths }))
        .collect())
}

#[must_use]
pub fn portability_key(path: &PortableRelativePath) -> String {
    path.as_str().nfc().flat_map(char::to_lowercase).collect()
}

/// Find the nearest ancestor containing `.kb/config.yml`.
///
/// # Errors
///
/// Returns [`KbError`] when no containing Vault can be found.
pub fn find_vault_root(start: &Path) -> Result<PathBuf, KbError> {
    let first = if start.is_file() {
        start.parent().unwrap_or(start)
    } else {
        start
    };
    for candidate in first.ancestors() {
        if candidate.join(".kb/config.yml").is_file() {
            return Ok(candidate.to_path_buf());
        }
    }
    Err(KbError::new(
        ErrorCode::VaultNotFound,
        format!("No Knowledge-Brain Vault contains {}", start.display()),
        false,
        "Pass --vault with a Vault path or run kb init.",
    ))
}

fn validate_portable_text(value: &str) -> Result<(), KbError> {
    if value.is_empty() || value.starts_with('/') || value.contains('\\') {
        return Err(unsafe_path(
            Path::new(value),
            "path must be a non-empty portable relative path",
        ));
    }

    for component in value.split('/') {
        if component.is_empty() || component == "." || component == ".." {
            return Err(unsafe_path(
                Path::new(value),
                "path contains an empty or relative component",
            ));
        }
        if component.ends_with([' ', '.']) {
            return Err(unsafe_path(
                Path::new(value),
                "path component ends with a space or period",
            ));
        }
        if component
            .chars()
            .any(|character| character.is_control() || r#"<>:"|?*"#.contains(character))
        {
            return Err(unsafe_path(
                Path::new(value),
                "path contains a cross-platform forbidden character",
            ));
        }

        let basename = component.split('.').next().unwrap_or(component);
        if WINDOWS_RESERVED_BASENAMES
            .iter()
            .any(|reserved| basename.eq_ignore_ascii_case(reserved))
        {
            return Err(unsafe_path(
                Path::new(value),
                "path uses a Windows reserved basename",
            ));
        }
    }
    Ok(())
}

fn unsafe_path(path: &Path, reason: &str) -> KbError {
    KbError::new(
        ErrorCode::UnsafePath,
        format!("Unsafe portable path {}: {reason}", path.display()),
        false,
        "Choose a relative path that is valid on Windows, macOS, and Linux.",
    )
}
