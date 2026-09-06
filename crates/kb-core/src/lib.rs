//! Domain types and deterministic rules shared by every Knowledge-Brain entry point.

mod admission;
mod config;
mod error;
mod operation;
mod path;
mod platform;
mod version;

pub use admission::{AdmissionDocument, AdmissionEntry};
pub use config::{
    ConfigSource, EffectiveConfig, EffectiveFiles, EffectiveLimits, EffectiveOperations,
    EffectiveSearch, PartialConfig, PartialFiles, PartialLimits, PartialOperations, PartialSearch,
    SearchMode, Sourced,
};
pub use error::{ErrorCode, KbError};
pub use operation::{
    AdoptionPlan, ObservedEntry, ObservedKind, OperationId, OperationKind, PlannedFile,
};
pub use path::{
    PortabilityCollision, PortableRelativePath, detect_portability_collisions, find_vault_root,
    portability_key, validate_admission_directory, validate_generated_path,
};
pub use platform::ensure_not_link_or_reparse_point;
pub use version::{
    CURRENT_SCHEMA_VERSION, SchemaCompatibility, SchemaVersion, SchemaVersionParseError,
};
