use serde::{Deserialize, Serialize};

/// Converts already-bounded source bytes into searchable text without filesystem access.
pub trait Extractor {
    fn id(&self) -> &'static str;
    fn version(&self) -> &'static str;
    fn supports(&self, media: MediaType) -> bool;
    fn extract(&self, media: MediaType, bytes: &[u8]) -> ExtractedDocument;
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaType {
    Markdown,
    PlainText,
    Yaml,
    Json,
    Csv,
    Pdf,
    Other,
}
impl MediaType {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Markdown => "markdown",
            Self::PlainText => "plain_text",
            Self::Yaml => "yaml",
            Self::Json => "json",
            Self::Csv => "csv",
            Self::Pdf => "pdf",
            Self::Other => "other",
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExtractionStatus {
    TextReady,
    MetadataOnly,
    Unsupported,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtractedBlock {
    pub heading: Option<String>,
    pub text: String,
    pub line_start: Option<u64>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtractedDocument {
    pub status: ExtractionStatus,
    pub extractor_id: String,
    pub extractor_version: String,
    pub blocks: Vec<ExtractedBlock>,
    pub warnings: Vec<String>,
}
