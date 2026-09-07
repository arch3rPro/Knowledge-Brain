use crate::{ErrorCode, KbError, PortableRelativePath, SchemaVersion, SourceLocation};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchScope {
    Wiki,
    Sources,
    All,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchRequest {
    pub query: String,
    pub scope: SearchScope,
    pub limit: usize,
    pub strict_backend: bool,
}
impl SearchRequest {
    /// Validate query text and bounded result count.
    /// # Errors
    /// Returns `invalid_query` for blank text or limits outside 1..=100.
    pub fn validate(&self) -> Result<(), KbError> {
        if self.query.trim().is_empty() || !(1..=100).contains(&self.limit) {
            return Err(KbError::new(
                ErrorCode::InvalidQuery,
                "Query must be nonblank and limit must be 1..=100.",
                false,
                "Provide query text and a valid --limit.",
            ));
        }
        Ok(())
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchHit {
    pub path: PortableRelativePath,
    pub content_path: PortableRelativePath,
    pub source_uri: Option<String>,
    pub title: String,
    pub heading: Option<String>,
    pub line_start: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<SourceLocation>,
    pub snippet: String,
    pub match_count: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backend: Option<SearchBackend>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub score_micros: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub explanation: Option<SearchExplanation>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchBackend {
    Direct,
    Bm25f,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchExplanation {
    pub query_terms: Vec<String>,
    pub fields: Vec<SearchFieldContribution>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchFieldContribution {
    pub field: SearchField,
    pub terms: Vec<String>,
    pub score_micros: u64,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchField {
    Title,
    Aliases,
    Heading,
    Tags,
    Body,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchGroup {
    pub scope: SearchScope,
    pub results: Vec<SearchHit>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResponse {
    pub schema_version: SchemaVersion,
    pub query: String,
    pub groups: Vec<SearchGroup>,
    pub warnings: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogEntry {
    pub scope: SearchScope,
    pub path: PortableRelativePath,
    pub sha256: String,
    pub title: String,
    pub headings: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Catalog {
    pub schema_version: SchemaVersion,
    pub indexer_version: String,
    pub entries: Vec<CatalogEntry>,
}
