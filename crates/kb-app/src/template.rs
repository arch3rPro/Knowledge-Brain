use kb_core::SchemaVersion;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

pub const ADMISSION_YAML: &str = include_str!("../../../assets/vault-template/admission.yml");
pub const KB_MD: &str = include_str!("../../../assets/vault-template/KB.md");
pub const LEGACY_KB_MD_V1_0: &str =
    include_str!("../../../assets/vault-template-history/v1.0/KB.md");
pub const RELEASED_KB_MD_V0_1_X: &str =
    include_str!("../../../assets/vault-template-history/released-v0.1.x/KB.md");
pub const HISTORICAL_KB_MD_V1_1: &str =
    include_str!("../../../assets/vault-template-history/v1.1/KB.md");
pub const HISTORICAL_TEMPLATE_MANIFEST_V1_1: &str =
    include_str!("../../../assets/vault-template-history/v1.1/template.yml");
pub const WIKI_INDEX_MD: &str = include_str!("../../../assets/vault-template/Wiki/index.md");
pub const WIKI_LOG_MD: &str = include_str!("../../../assets/vault-template/Wiki/log.md");
pub const ADMISSION_SCHEMA_JSON: &str = include_str!("../../../schemas/admission.schema.json");
pub const CONFIG_SCHEMA_JSON: &str = include_str!("../../../schemas/config.schema.json");
pub const VAULT_TEMPLATE_VERSION: SchemaVersion = SchemaVersion::new(1, 2);
pub const RULES_START_MARKER: &str = "<!-- kb:rules:start -->";
pub const RULES_END_MARKER: &str = "<!-- kb:rules:end -->";

pub const EMPTY_DIRECTORIES: [&str; 5] = [
    "Wiki/external-sources/.objects/sha256",
    "Wiki/research",
    "Wiki/articles",
    ".kb/cache",
    ".kb/runtime",
];

pub const STATIC_FILES: [(&str, &str); 6] = [
    ("admission.yml", ADMISSION_YAML),
    ("KB.md", KB_MD),
    ("Wiki/index.md", WIKI_INDEX_MD),
    ("Wiki/log.md", WIKI_LOG_MD),
    (".kb/schemas/admission.schema.json", ADMISSION_SCHEMA_JSON),
    (".kb/schemas/config.schema.json", CONFIG_SCHEMA_JSON),
];

#[must_use]
pub fn config_yaml(vault_id: Uuid) -> String {
    format!("schema_version: \"v1.0\"\nvault_id: \"{vault_id}\"\n\nsearch:\n  mode: direct\n")
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct TemplateManifest {
    pub schema_version: SchemaVersion,
    pub template_version: SchemaVersion,
    pub managed: Vec<ManagedTemplate>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ManagedTemplate {
    pub path: String,
    pub ownership: ManagedTemplateOwnership,
    pub sha256: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ManagedTemplateOwnership {
    MarkedRegion,
    WholeFile,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ManagedRegion<'a> {
    pub content: &'a str,
    pub content_start: usize,
    pub content_end: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MarkerError {
    Missing,
    Duplicate,
    Reversed,
}

#[must_use]
pub fn template_manifest_yaml() -> String {
    serde_yaml_ng::to_string(&current_template_manifest())
        .expect("static template manifest serializes")
}

#[must_use]
pub(crate) fn current_template_manifest() -> TemplateManifest {
    TemplateManifest {
        schema_version: kb_core::CURRENT_SCHEMA_VERSION,
        template_version: VAULT_TEMPLATE_VERSION,
        managed: vec![
            ManagedTemplate {
                path: "KB.md".into(),
                ownership: ManagedTemplateOwnership::MarkedRegion,
                sha256: hash(managed_rules(KB_MD).unwrap_or_default().as_bytes()),
            },
            ManagedTemplate {
                path: ".kb/schemas/admission.schema.json".into(),
                ownership: ManagedTemplateOwnership::WholeFile,
                sha256: hash(ADMISSION_SCHEMA_JSON.as_bytes()),
            },
            ManagedTemplate {
                path: ".kb/schemas/config.schema.json".into(),
                ownership: ManagedTemplateOwnership::WholeFile,
                sha256: hash(CONFIG_SCHEMA_JSON.as_bytes()),
            },
        ],
    }
}

pub(crate) fn managed_rules(content: &str) -> Option<&str> {
    parse_managed_rules(content)
        .ok()
        .map(|region| region.content)
}

pub(crate) fn parse_managed_rules(content: &str) -> Result<ManagedRegion<'_>, MarkerError> {
    let starts = content
        .match_indices(RULES_START_MARKER)
        .collect::<Vec<_>>();
    let ends = content.match_indices(RULES_END_MARKER).collect::<Vec<_>>();
    if starts.is_empty() || ends.is_empty() {
        return Err(MarkerError::Missing);
    }
    if starts.len() != 1 || ends.len() != 1 {
        return Err(MarkerError::Duplicate);
    }
    let content_start = starts[0].0 + RULES_START_MARKER.len();
    let content_end = ends[0].0;
    if content_end < content_start {
        return Err(MarkerError::Reversed);
    }
    Ok(ManagedRegion {
        content: &content[content_start..content_end],
        content_start,
        content_end,
    })
}

pub(crate) fn hash(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
