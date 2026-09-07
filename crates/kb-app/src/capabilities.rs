use kb_core::{CURRENT_SCHEMA_VERSION, SchemaVersion};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
// These independent booleans are the stable machine contract clients consume.
#[allow(clippy::struct_excessive_bools)]
pub struct Capabilities {
    pub schema_version: SchemaVersion,
    pub direct_search: bool,
    pub bm25: bool,
    pub extractors: Vec<String>,
    pub mcp: bool,
    pub http: bool,
    pub commands: Vec<&'static str>,
}

#[must_use]
pub fn capabilities() -> Capabilities {
    Capabilities {
        schema_version: CURRENT_SCHEMA_VERSION,
        direct_search: true,
        bm25: false,
        extractors: vec![
            "builtin-text".into(),
            "builtin-html".into(),
            "builtin-epub".into(),
            "builtin-docx".into(),
        ],
        mcp: false,
        http: false,
        commands: vec![
            "init",
            "adopt",
            "apply",
            "review",
            "query",
            "lint",
            "cache",
            "source",
            "operation",
            "config",
            "status",
            "doctor",
            "vault",
            "paths",
            "version",
            "capabilities",
        ],
    }
}
