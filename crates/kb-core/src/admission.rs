use std::{collections::BTreeSet, path::Path};

use serde::Deserialize;

use crate::{
    CURRENT_SCHEMA_VERSION, ErrorCode, KbError, SchemaVersion, ensure_not_link_or_reparse_point,
    portability_key, validate_admission_directory,
};

const DEFAULT_INCLUDE: [&str; 3] = ["**/*.md", "**/*.txt", "**/*.pdf"];
const DEFAULT_EXCLUDE: [&str; 2] = ["**/.git/**", "**/.kb/**"];

#[derive(Debug, Clone, Deserialize)]
pub struct AdmissionDocument {
    pub schema_version: SchemaVersion,
    pub directories: Vec<AdmissionEntry>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AdmissionEntry {
    pub id: String,
    pub path: String,
    pub enabled: bool,
    pub include: Option<Vec<String>>,
    pub exclude: Option<Vec<String>>,
}

impl AdmissionDocument {
    /// Validate admission identities, portable paths, and filesystem boundaries.
    ///
    /// # Errors
    ///
    /// Returns [`KbError`] for an unsupported schema, duplicate ID or path,
    /// missing directory, non-directory, symbolic link, junction, or unsafe path.
    pub fn validate(&self, vault_root: &Path) -> Result<(), KbError> {
        if self.schema_version != CURRENT_SCHEMA_VERSION {
            return Err(KbError::invalid_config(
                "admission.yml",
                format!("unsupported schema version {}", self.schema_version),
            ));
        }

        let mut ids = BTreeSet::new();
        let mut paths = BTreeSet::new();
        let mut validated = Vec::with_capacity(self.directories.len());
        for entry in &self.directories {
            if entry.id.trim().is_empty() || !ids.insert(entry.id.clone()) {
                return Err(KbError::invalid_config(
                    "admission.yml",
                    format!("admission id is empty or duplicated: {}", entry.id),
                ));
            }
            let portable = validate_admission_directory(Path::new(&entry.path))?;
            if !paths.insert(portability_key(&portable)) {
                return Err(KbError::invalid_config(
                    "admission.yml",
                    format!(
                        "admission path is duplicated across platforms: {}",
                        entry.path
                    ),
                ));
            }
            validated.push(portable);
        }

        for portable in validated {
            let path = vault_root.join(portable.to_native_path());
            ensure_not_link_or_reparse_point(&path)?;
            if !path.is_dir() {
                return Err(KbError::new(
                    ErrorCode::PathNotAdmitted,
                    format!("Admission path is not a directory: {}", path.display()),
                    false,
                    "Create the directory or remove it from admission.yml.",
                ));
            }
        }
        Ok(())
    }
}

impl AdmissionEntry {
    #[must_use]
    pub fn effective_include(&self) -> Vec<&str> {
        self.include.as_ref().map_or_else(
            || DEFAULT_INCLUDE.to_vec(),
            |patterns| patterns.iter().map(String::as_str).collect(),
        )
    }

    #[must_use]
    pub fn effective_exclude(&self) -> Vec<&str> {
        self.exclude.as_ref().map_or_else(
            || DEFAULT_EXCLUDE.to_vec(),
            |patterns| patterns.iter().map(String::as_str).collect(),
        )
    }
}
