use uuid::Uuid;

pub const ADMISSION_YAML: &str = include_str!("../../../assets/vault-template/admission.yml");
pub const KB_MD: &str = include_str!("../../../assets/vault-template/KB.md");
pub const WIKI_INDEX_MD: &str = include_str!("../../../assets/vault-template/Wiki/index.md");
pub const WIKI_LOG_MD: &str = include_str!("../../../assets/vault-template/Wiki/log.md");
pub const ADMISSION_SCHEMA_JSON: &str = include_str!("../../../schemas/admission.schema.json");
pub const CONFIG_SCHEMA_JSON: &str = include_str!("../../../schemas/config.schema.json");

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
