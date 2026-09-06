use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{CURRENT_SCHEMA_VERSION, SchemaVersion};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfigSource {
    BuiltIn,
    User,
    Vault,
    VaultLocal,
    Environment,
    Cli,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Sourced<T> {
    pub value: T,
    pub source: ConfigSource,
}

impl<T> Sourced<T> {
    #[must_use]
    pub const fn new(value: T, source: ConfigSource) -> Self {
        Self { value, source }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchMode {
    Direct,
    Bm25,
}

impl fmt::Display for SearchMode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Direct => formatter.write_str("direct"),
            Self::Bm25 => formatter.write_str("bm25"),
        }
    }
}

impl FromStr for SearchMode {
    type Err = &'static str;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "direct" => Ok(Self::Direct),
            "bm25" => Ok(Self::Bm25),
            _ => Err("search mode must be direct or bm25"),
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct PartialConfig {
    pub schema_version: Option<SchemaVersion>,
    pub vault_id: Option<Uuid>,
    pub search: Option<PartialSearch>,
    pub limits: Option<PartialLimits>,
    pub files: Option<PartialFiles>,
    pub operations: Option<PartialOperations>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct PartialSearch {
    pub mode: Option<SearchMode>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct PartialLimits {
    pub max_file_bytes: Option<u64>,
    pub max_files_per_review: Option<u64>,
    pub max_total_read_bytes: Option<u64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct PartialFiles {
    pub include_hidden: Option<bool>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct PartialOperations {
    pub plan_retention_hours: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EffectiveConfig {
    pub schema_version: SchemaVersion,
    pub vault_id: Uuid,
    pub search: EffectiveSearch,
    pub limits: EffectiveLimits,
    pub files: EffectiveFiles,
    pub operations: EffectiveOperations,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EffectiveSearch {
    pub mode: Sourced<SearchMode>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EffectiveLimits {
    pub max_file_bytes: Sourced<u64>,
    pub max_files_per_review: Sourced<u64>,
    pub max_total_read_bytes: Sourced<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EffectiveFiles {
    pub include_hidden: Sourced<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EffectiveOperations {
    pub plan_retention_hours: Sourced<u64>,
}

impl EffectiveConfig {
    #[must_use]
    pub fn built_in(vault_id: Uuid) -> Self {
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            vault_id,
            search: EffectiveSearch {
                mode: Sourced::new(SearchMode::Direct, ConfigSource::BuiltIn),
            },
            limits: EffectiveLimits {
                max_file_bytes: Sourced::new(50 * 1024 * 1024, ConfigSource::BuiltIn),
                max_files_per_review: Sourced::new(10_000, ConfigSource::BuiltIn),
                max_total_read_bytes: Sourced::new(500 * 1024 * 1024, ConfigSource::BuiltIn),
            },
            files: EffectiveFiles {
                include_hidden: Sourced::new(false, ConfigSource::BuiltIn),
            },
            operations: EffectiveOperations {
                plan_retention_hours: Sourced::new(168, ConfigSource::BuiltIn),
            },
        }
    }

    pub fn apply(&mut self, partial: PartialConfig, source: ConfigSource) {
        if let Some(search) = partial.search {
            if let Some(value) = search.mode {
                self.search.mode = Sourced::new(value, source);
            }
        }
        if let Some(limits) = partial.limits {
            if let Some(value) = limits.max_file_bytes {
                self.limits.max_file_bytes = Sourced::new(value, source);
            }
            if let Some(value) = limits.max_files_per_review {
                self.limits.max_files_per_review = Sourced::new(value, source);
            }
            if let Some(value) = limits.max_total_read_bytes {
                self.limits.max_total_read_bytes = Sourced::new(value, source);
            }
        }
        if let Some(files) = partial.files {
            if let Some(value) = files.include_hidden {
                self.files.include_hidden = Sourced::new(value, source);
            }
        }
        if let Some(operations) = partial.operations {
            if let Some(value) = operations.plan_retention_hours {
                self.operations.plan_retention_hours = Sourced::new(value, source);
            }
        }
    }
}
