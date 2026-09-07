use std::{fmt, path::PathBuf, str::FromStr};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{PortableRelativePath, SchemaVersion};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct OperationId(Uuid);

impl OperationId {
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    #[must_use]
    pub const fn as_uuid(self) -> Uuid {
        self.0
    }
}

impl Default for OperationId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for OperationId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl FromStr for OperationId {
    type Err = uuid::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Uuid::parse_str(value).map(Self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationKind {
    AdoptVault,
    SaveKnowledge,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnowledgeChangeRequest {
    pub path: PortableRelativePath,
    pub before_sha256: Option<String>,
    pub summary: String,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnowledgePlanRequest {
    pub schema_version: SchemaVersion,
    pub changes: Vec<KnowledgeChangeRequest>,
}

impl KnowledgePlanRequest {
    /// Validate untrusted knowledge-plan input before any Vault reads or writes.
    ///
    /// # Errors
    ///
    /// Rejects incompatible schemas, unsafe targets, duplicate paths, malformed
    /// original hashes, multiline summaries, and configured count or size excesses.
    pub fn validate(&self, max_changes: u64, max_file_bytes: u64) -> Result<(), crate::KbError> {
        if self.schema_version != crate::CURRENT_SCHEMA_VERSION {
            return Err(crate::KbError::invalid_config(
                "knowledge request schema_version",
                format!("expected {}", crate::CURRENT_SCHEMA_VERSION),
            ));
        }
        if self.changes.is_empty() || self.changes.len() as u64 > max_changes {
            return Err(crate::KbError::invalid_config(
                "knowledge request changes",
                format!("expected 1..={max_changes} changes"),
            ));
        }
        let mut paths = std::collections::BTreeSet::new();
        for change in &self.changes {
            validate_knowledge_change(change, max_file_bytes)?;
            if !paths.insert(change.path.clone()) {
                return Err(crate::KbError::invalid_config(
                    "knowledge request changes",
                    format!("duplicate path {}", change.path.as_str()),
                ));
            }
        }
        Ok(())
    }
}

fn validate_knowledge_change(
    change: &KnowledgeChangeRequest,
    max_file_bytes: u64,
) -> Result<(), crate::KbError> {
    let path = change.path.as_str();
    let in_layer = path.starts_with("research/") || path.starts_with("articles/");
    let is_markdown = std::path::Path::new(path)
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("md"));
    let file_name = path.rsplit('/').next().unwrap_or_default();
    let reserved =
        file_name.eq_ignore_ascii_case("index.md") || file_name.eq_ignore_ascii_case("log.md");
    if !in_layer || !is_markdown || reserved {
        return Err(crate::KbError::invalid_config(
            "knowledge request path",
            "target must be a non-reserved Markdown path below research/ or articles/",
        ));
    }
    if change.before_sha256.as_deref().is_some_and(|digest| {
        digest.len() != 64
            || !digest
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    }) {
        return Err(crate::KbError::invalid_config(
            "knowledge request before_sha256",
            "expected null or 64 lowercase hexadecimal characters",
        ));
    }
    if change.summary.trim().is_empty() || change.summary.lines().count() != 1 {
        return Err(crate::KbError::invalid_config(
            "knowledge request summary",
            "summary must be non-empty and contain one line",
        ));
    }
    if change.content.is_empty() || change.content.len() as u64 > max_file_bytes {
        return Err(crate::KbError::invalid_config(
            "knowledge request content",
            format!("content must be 1..={max_file_bytes} bytes"),
        ));
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnowledgeWrite {
    pub path: PortableRelativePath,
    pub before_sha256: Option<String>,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnowledgePlan {
    pub schema_version: SchemaVersion,
    pub operation_id: OperationId,
    pub kind: OperationKind,
    pub vault_id: Uuid,
    pub target: PathBuf,
    pub changes: Vec<KnowledgeChangeRequest>,
    pub source_versions: Vec<String>,
    pub admission_sha256: String,
    pub config_sha256: String,
    pub writes: Vec<KnowledgeWrite>,
    pub diff: String,
    pub created_at: String,
    pub app_version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnowledgePlanResult {
    pub kind: OperationKind,
    pub operation_id: OperationId,
    pub vault_id: Uuid,
    pub target: PathBuf,
    pub changed: Vec<PortableRelativePath>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObservedKind {
    File,
    Directory,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObservedEntry {
    pub relative_path: PortableRelativePath,
    pub kind: ObservedKind,
    pub size: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlannedFile {
    pub relative_path: PortableRelativePath,
    pub size: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdoptionPlan {
    pub schema_version: SchemaVersion,
    pub operation_id: OperationId,
    pub kind: OperationKind,
    pub vault_id: Uuid,
    pub target: PathBuf,
    pub observed_entries: Vec<ObservedEntry>,
    pub creates: Vec<PlannedFile>,
    pub created_at: String,
    pub app_version: String,
}
