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
    Html,
    Epub,
    Docx,
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
            Self::Html => "html",
            Self::Epub => "epub",
            Self::Docx => "docx",
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<SourceLocation>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SourceLocation {
    Html {
        block: u64,
    },
    Epub {
        resource: String,
        block: u64,
    },
    Docx {
        paragraph: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        table: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        row: Option<u64>,
    },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtractedLink {
    pub text: Option<String>,
    pub target: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<SourceLocation>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtractedDocument {
    pub status: ExtractionStatus,
    pub extractor_id: String,
    pub extractor_version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub links: Vec<ExtractedLink>,
    pub blocks: Vec<ExtractedBlock>,
    pub warnings: Vec<String>,
}
