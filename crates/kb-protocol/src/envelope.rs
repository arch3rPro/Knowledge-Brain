use kb_core::{CURRENT_SCHEMA_VERSION, ErrorCode, KbError, SchemaVersion};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct Envelope<T> {
    pub schema_version: SchemaVersion,
    pub data: T,
}

impl<T> Envelope<T> {
    #[must_use]
    pub const fn new(data: T) -> Self {
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            data,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ErrorEnvelope {
    pub schema_version: SchemaVersion,
    pub error: ErrorBody,
}

#[derive(Debug, Serialize)]
pub struct ErrorBody {
    pub code: ErrorCode,
    pub message: String,
    pub retryable: bool,
    pub next_action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

impl From<KbError> for ErrorEnvelope {
    fn from(error: KbError) -> Self {
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            error: ErrorBody {
                code: error.code,
                message: error.message,
                retryable: error.retryable,
                next_action: error.next_action,
                details: error.details,
            },
        }
    }
}
