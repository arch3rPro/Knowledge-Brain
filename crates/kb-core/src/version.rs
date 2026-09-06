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
    pub const fn compatibility_with(self, current: Self) -> SchemaCompatibility {
        if self.major == current.major && self.minor == current.minor {
            SchemaCompatibility::Current
        } else if self.major < current.major
            || (self.major == current.major && self.minor < current.minor)
        {
            SchemaCompatibility::OlderMigratable
        } else if self.major == current.major {
            SchemaCompatibility::NewerMinorReadOnly
        } else {
            SchemaCompatibility::NewerMajorDiagnosticOnly
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
    NewerMinorReadOnly,
    NewerMajorDiagnosticOnly,
}
