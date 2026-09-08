use kb_core::{
    MediaType, OperationId, PortableRelativePath, SchemaVersion, SourceId, SourceVersion,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureKind {
    CaptureSources,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceInput {
    pub version: SourceVersion,
    pub relative_path: PortableRelativePath,
    pub media_type: MediaType,
    pub present: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordWrite {
    pub path: PortableRelativePath,
    pub before_sha256: Option<String>,
    pub content: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceCapturePlan {
    pub schema_version: SchemaVersion,
    pub operation_id: OperationId,
    pub kind: CaptureKind,
    pub vault_id: Uuid,
    pub target: PathBuf,
    pub admission_sha256: String,
    pub config_sha256: String,
    pub inputs: Vec<SourceInput>,
    pub writes: Vec<RecordWrite>,
    pub created_at: String,
    pub app_version: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceCaptureResult {
    pub kind: CaptureKind,
    pub operation_id: OperationId,
    pub vault_id: Uuid,
    pub target: PathBuf,
    #[serde(default)]
    pub source_paths: Vec<PortableRelativePath>,
    pub captured: Vec<String>,
    pub marked_missing: Vec<String>,
    pub warnings: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeKind {
    Added,
    Modified,
    Deleted,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceChange {
    pub kind: ChangeKind,
    pub source: SourceId,
    pub previous_sha256: Option<String>,
    pub current_sha256: Option<String>,
    pub possible_move_from: Vec<SourceId>,
    pub extraction: Option<kb_core::ExtractedDocument>,
}
#[derive(Debug, Clone, Serialize)]
pub struct ReviewReport {
    pub schema_version: SchemaVersion,
    pub vault_id: Uuid,
    pub operation_id: Option<OperationId>,
    pub changes: Vec<SourceChange>,
    pub unchanged: usize,
    pub skipped: Vec<crate::SkippedSource>,
}
