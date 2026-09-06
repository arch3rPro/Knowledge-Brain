use serde::{Deserialize, Serialize};
use serde_json::json;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    InvalidConfig,
    PathNotAdmitted,
    PlanStale,
    WriteBusy,
    VaultNeedsRecovery,
    RestoreFailed,
    IndexStale,
    CapabilityUnavailable,
    AuthDenied,
    VaultNotFound,
    TargetNotEmpty,
    UnsafePath,
    OperationNotFound,
    IoFailure,
}

/// A caller-facing application error with stable machine semantics.
#[derive(Debug, Clone, Error)]
#[error("{message}")]
pub struct KbError {
    pub code: ErrorCode,
    pub message: String,
    pub retryable: bool,
    pub next_action: String,
    pub details: Option<serde_json::Value>,
}

impl KbError {
    #[must_use]
    pub fn new(
        code: ErrorCode,
        message: impl Into<String>,
        retryable: bool,
        next_action: impl Into<String>,
    ) -> Self {
        Self {
            code,
            message: message.into(),
            retryable,
            next_action: next_action.into(),
            details: None,
        }
    }

    #[must_use]
    pub fn with_details(mut self, details: serde_json::Value) -> Self {
        self.details = Some(details);
        self
    }

    #[must_use]
    pub fn invalid_config(path: impl Into<String>, reason: impl Into<String>) -> Self {
        let path = path.into();
        let reason = reason.into();
        Self::new(
            ErrorCode::InvalidConfig,
            format!("Invalid configuration in {path}: {reason}"),
            false,
            format!("Correct {path} and run the command again."),
        )
        .with_details(json!({ "path": path, "reason": reason }))
    }

    #[must_use]
    pub fn io_failure(
        action: impl Into<String>,
        path: impl Into<String>,
        reason: impl Into<String>,
    ) -> Self {
        let action = action.into();
        let path = path.into();
        let reason = reason.into();
        Self::new(
            ErrorCode::IoFailure,
            format!("Cannot {action} {path}: {reason}"),
            false,
            "Check the path, permissions, and available storage, then run the command again.",
        )
        .with_details(json!({ "action": action, "path": path, "reason": reason }))
    }
}
