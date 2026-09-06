//! Domain types and deterministic rules shared by every Knowledge-Brain entry point.

mod error;
mod version;

pub use error::{ErrorCode, KbError};
pub use version::{
    CURRENT_SCHEMA_VERSION, SchemaCompatibility, SchemaVersion, SchemaVersionParseError,
};
