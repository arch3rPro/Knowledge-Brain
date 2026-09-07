use std::{fmt, str::FromStr};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use thiserror::Error;

/// The current on-disk and machine-response schema understood by this build.
pub const CURRENT_SCHEMA_VERSION: SchemaVersion = SchemaVersion::new(1, 0);

/// A Knowledge-Brain schema version encoded as `v<major>.<minor>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SchemaVersion {
    major: u16,
    minor: u16,
}

impl SchemaVersion {
    #[must_use]
    pub const fn new(major: u16, minor: u16) -> Self {
        Self { major, minor }
    }

    #[must_use]
    pub const fn major(self) -> u16 {
        self.major
    }

    #[must_use]
    pub const fn minor(self) -> u16 {
        self.minor
    }

    #[must_use]
    pub const fn relation_to(self, current: Self) -> SchemaRelation {
        if self.major == current.major && self.minor == current.minor {
            SchemaRelation::Current
        } else if self.major < current.major
            || (self.major == current.major && self.minor < current.minor)
        {
            SchemaRelation::Older
        } else if self.major == current.major {
            SchemaRelation::NewerMinor
        } else {
            SchemaRelation::NewerMajor
        }
    }

    #[must_use]
    pub const fn compatibility_with(self, current: Self) -> SchemaCompatibility {
        match self.relation_to(current) {
            SchemaRelation::Current => SchemaCompatibility::Current,
            SchemaRelation::Older => SchemaCompatibility::OlderMigratable,
            SchemaRelation::NewerMinor => SchemaCompatibility::NewerMinorReadOnly,
            SchemaRelation::NewerMajor => SchemaCompatibility::NewerMajorDiagnosticOnly,
        }
    }
}

impl fmt::Display for SchemaVersion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "v{}.{}", self.major, self.minor)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("schema version must use canonical v<major>.<minor> form")]
pub struct SchemaVersionParseError;

impl FromStr for SchemaVersion {
    type Err = SchemaVersionParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let numeric = value.strip_prefix('v').ok_or(SchemaVersionParseError)?;
        let (major, minor) = numeric.split_once('.').ok_or(SchemaVersionParseError)?;
        if major.is_empty() || minor.is_empty() || minor.contains('.') {
            return Err(SchemaVersionParseError);
        }

        let version = Self::new(
            major.parse().map_err(|_| SchemaVersionParseError)?,
            minor.parse().map_err(|_| SchemaVersionParseError)?,
        );
        if version.to_string() != value {
            return Err(SchemaVersionParseError);
        }
        Ok(version)
    }
}

impl Serialize for SchemaVersion {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for SchemaVersion {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        raw.parse().map_err(de::Error::custom)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SchemaCompatibility {
    Current,
    OlderMigratable,
    OlderUnsupported,
    NewerMinorReadOnly,
    NewerMajorDiagnosticOnly,
}

/// The numeric relation between a vault schema and the schema understood by this build.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchemaRelation {
    Current,
    Older,
    NewerMinor,
    NewerMajor,
}

/// A registered migration from one schema version to a strictly newer schema version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct MigrationStep {
    pub from: SchemaVersion,
    pub to: SchemaVersion,
}

impl MigrationStep {
    #[must_use]
    pub const fn new(from: SchemaVersion, to: SchemaVersion) -> Self {
        Self { from, to }
    }
}

/// A rejected migration catalog definition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum MigrationCatalogError {
    #[error("migration step must advance schema version: {from} -> {to}")]
    NonForwardStep {
        from: SchemaVersion,
        to: SchemaVersion,
    },
    #[error("migration catalog has multiple steps from {from}")]
    DuplicateSource { from: SchemaVersion },
}

/// A deterministic set of directed schema migration steps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationCatalog {
    steps: Vec<MigrationStep>,
}

impl MigrationCatalog {
    /// # Errors
    ///
    /// Returns an error for a self step, a backward step, or duplicate source steps.
    pub fn new(
        steps: impl IntoIterator<Item = MigrationStep>,
    ) -> Result<Self, MigrationCatalogError> {
        let mut steps: Vec<_> = steps.into_iter().collect();
        steps.sort_unstable_by_key(|step| step.from);

        for step in &steps {
            if step.from >= step.to {
                return Err(MigrationCatalogError::NonForwardStep {
                    from: step.from,
                    to: step.to,
                });
            }
        }

        for pair in steps.windows(2) {
            if pair[0].from == pair[1].from {
                return Err(MigrationCatalogError::DuplicateSource { from: pair[0].from });
            }
        }

        Ok(Self { steps })
    }

    #[must_use]
    pub const fn empty() -> Self {
        Self { steps: Vec::new() }
    }

    #[must_use]
    pub fn classify(&self, version: SchemaVersion, current: SchemaVersion) -> SchemaCompatibility {
        match version.relation_to(current) {
            SchemaRelation::Current => SchemaCompatibility::Current,
            SchemaRelation::Older => {
                if self.has_path_to_current(version, current) {
                    SchemaCompatibility::OlderMigratable
                } else {
                    SchemaCompatibility::OlderUnsupported
                }
            }
            SchemaRelation::NewerMinor => SchemaCompatibility::NewerMinorReadOnly,
            SchemaRelation::NewerMajor => SchemaCompatibility::NewerMajorDiagnosticOnly,
        }
    }

    fn has_path_to_current(&self, mut version: SchemaVersion, current: SchemaVersion) -> bool {
        for _ in 0..self.steps.len() {
            let Ok(index) = self.steps.binary_search_by_key(&version, |step| step.from) else {
                return false;
            };
            version = self.steps[index].to;
            if version == current {
                return true;
            }
        }
        false
    }
}
