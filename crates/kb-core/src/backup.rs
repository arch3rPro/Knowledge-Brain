use std::{collections::BTreeMap, path::PathBuf};

use serde::{Deserialize, Serialize};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use uuid::Uuid;

use crate::{
    CURRENT_SCHEMA_VERSION, KbError, PortableRelativePath, SchemaVersion, portability_key,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackupFile {
    pub path: PortableRelativePath,
    pub size: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackupManifest {
    pub schema_version: SchemaVersion,
    pub vault_schema_version: SchemaVersion,
    pub vault_id: Uuid,
    pub created_at: String,
    pub app_version: String,
    pub complete_source_evidence: bool,
    pub directories: Vec<PortableRelativePath>,
    pub files: Vec<BackupFile>,
}

impl BackupManifest {
    /// Validate the portable, deterministic backup manifest contract.
    ///
    /// # Errors
    ///
    /// Returns an invalid-configuration error for unsupported versions,
    /// malformed metadata, duplicate paths, collisions, or invalid hashes.
    pub fn validate(&self) -> Result<(), KbError> {
        if self.schema_version != CURRENT_SCHEMA_VERSION {
            return Err(invalid("unsupported manifest schema version"));
        }
        if self.app_version.trim().is_empty()
            || OffsetDateTime::parse(&self.created_at, &Rfc3339).is_err()
        {
            return Err(invalid("invalid creation metadata"));
        }
        if !self.directories.windows(2).all(|pair| pair[0] < pair[1])
            || !self
                .files
                .windows(2)
                .all(|pair| pair[0].path < pair[1].path)
        {
            return Err(invalid("paths must be strictly sorted and unique"));
        }

        let mut paths = BTreeMap::new();
        for directory in &self.directories {
            insert_path(&mut paths, directory, "directory")?;
        }
        for file in &self.files {
            if file.sha256.len() != 64
                || !file
                    .sha256
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            {
                return Err(invalid(
                    "file SHA-256 must be 64 lowercase hexadecimal characters",
                ));
            }
            insert_path(&mut paths, &file.path, "file")?;
        }
        for required in ["Wiki", ".kb", ".kb/schemas"] {
            if !self
                .directories
                .iter()
                .any(|path| path.as_str() == required)
            {
                return Err(invalid(&format!(
                    "required directory is missing: {required}"
                )));
            }
        }
        for required in ["admission.yml", "KB.md", ".kb/config.yml"] {
            if !self.files.iter().any(|file| file.path.as_str() == required) {
                return Err(invalid(&format!("required file is missing: {required}")));
            }
        }
        self.files.iter().try_fold(0_u64, |total, file| {
            total
                .checked_add(file.size)
                .ok_or_else(|| invalid("total file size overflow"))
        })?;
        for file in &self.files {
            let prefix = format!("{}/", file.path.as_str());
            if self
                .directories
                .iter()
                .any(|path| path.as_str().starts_with(&prefix))
                || self.files.iter().any(|other| {
                    other.path != file.path && other.path.as_str().starts_with(&prefix)
                })
            {
                return Err(invalid("a file path cannot contain child entries"));
            }
        }
        Ok(())
    }

    #[must_use]
    pub fn total_bytes(&self) -> u64 {
        self.files
            .iter()
            .fold(0_u64, |total, file| total.saturating_add(file.size))
    }
}

fn insert_path(
    paths: &mut BTreeMap<String, (&'static str, String)>,
    path: &PortableRelativePath,
    kind: &'static str,
) -> Result<(), KbError> {
    let key = portability_key(path);
    if let Some((prior_kind, prior)) = paths.insert(key, (kind, path.as_str().to_owned())) {
        return Err(invalid(&format!(
            "portable path collision between {prior_kind} {prior} and {kind} {}",
            path.as_str()
        )));
    }
    Ok(())
}

fn invalid(reason: &str) -> KbError {
    KbError::invalid_config("manifest.json", reason)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackupCreateReport {
    pub archive: PathBuf,
    pub vault_id: Uuid,
    pub complete_source_evidence: bool,
    pub file_count: u64,
    pub total_bytes: u64,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackupVerifyReport {
    pub archive: PathBuf,
    pub vault_schema_version: SchemaVersion,
    pub vault_id: Uuid,
    pub complete_source_evidence: bool,
    pub file_count: u64,
    pub total_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackupRestoreReport {
    pub archive: PathBuf,
    pub target: PathBuf,
    pub vault_schema_version: SchemaVersion,
    pub vault_id: Uuid,
    pub complete_source_evidence: bool,
    pub file_count: u64,
    pub total_bytes: u64,
    pub warnings: Vec<String>,
}
