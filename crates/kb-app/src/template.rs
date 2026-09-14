use kb_core::SchemaVersion;
use serde::Serialize;
use sha2::{Digest, Sha256};
use uuid::Uuid;

pub const ADMISSION_YAML: &str = include_str!("../../../assets/vault-template/admission.yml");
pub const KB_MD: &str = include_str!("../../../assets/vault-template/KB.md");
pub const LEGACY_KB_MD_V1_0: &str =
    include_str!("../../../assets/vault-template-history/v1.0/KB.md");
pub const RELEASED_KB_MD_V0_1_X: &str =
    include_str!("../../../assets/vault-template-history/released-v0.1.x/KB.md");
pub const WIKI_INDEX_MD: &str = include_str!("../../../assets/vault-template/Wiki/index.md");
pub const WIKI_LOG_MD: &str = include_str!("../../../assets/vault-template/Wiki/log.md");
pub const ADMISSION_SCHEMA_JSON: &str = include_str!("../../../schemas/admission.schema.json");
pub const CONFIG_SCHEMA_JSON: &str = include_str!("../../../schemas/config.schema.json");
pub const VAULT_TEMPLATE_VERSION: SchemaVersion = SchemaVersion::new(1, 1);

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

#[derive(Serialize)]
struct TemplateManifest<'a> {
    schema_version: SchemaVersion,
    template_version: SchemaVersion,
    managed: [ManagedTemplate<'a>; 3],
}

#[derive(Serialize)]
struct ManagedTemplate<'a> {
    path: &'a str,
    ownership: &'a str,
    sha256: String,
}

#[must_use]
pub fn template_manifest_yaml() -> String {
    let manifest = TemplateManifest {
        schema_version: kb_core::CURRENT_SCHEMA_VERSION,
        template_version: VAULT_TEMPLATE_VERSION,
        managed: [
            ManagedTemplate {
                path: "KB.md",
                ownership: "marked_region",
                sha256: hash(managed_rules(KB_MD).unwrap_or_default().as_bytes()),
            },
            ManagedTemplate {
                path: ".kb/schemas/admission.schema.json",
                ownership: "whole_file",
                sha256: hash(ADMISSION_SCHEMA_JSON.as_bytes()),
            },
            ManagedTemplate {
                path: ".kb/schemas/config.schema.json",
                ownership: "whole_file",
                sha256: hash(CONFIG_SCHEMA_JSON.as_bytes()),
            },
        ],
    };
    serde_yaml_ng::to_string(&manifest).expect("static template manifest serializes")
}

pub(crate) fn managed_rules(content: &str) -> Option<&str> {
    const START: &str = "<!-- kb:rules:start -->";
    const END: &str = "<!-- kb:rules:end -->";
    let start = content.find(START)? + START.len();
    let end = content[start..].find(END)? + start;
    Some(&content[start..end])
}

fn hash(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
