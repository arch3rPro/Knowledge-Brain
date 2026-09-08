use semver::Version;
use serde_json::Value;

use crate::{ReleaseTarget, UpdateError};

/// Validates the machine-readable identity reported by a staged executable.
///
/// # Errors
///
/// Returns [`UpdateError::VerificationFailed`] unless the executable reports
/// the exact release version and target and identifies itself as official.
pub fn validate_staged_identity(
    output: &[u8],
    expected_version: &Version,
    expected_target: ReleaseTarget,
) -> Result<(), UpdateError> {
    let response: Value = serde_json::from_slice(output).map_err(|error| {
        UpdateError::VerificationFailed(format!("parse staged executable identity: {error}"))
    })?;
    let data = response.get("data").ok_or_else(|| {
        UpdateError::VerificationFailed("staged executable response has no data".into())
    })?;
    let version = data.get("app_version").and_then(Value::as_str);
    let official = data
        .pointer("/distribution/official_release")
        .and_then(Value::as_bool);
    let target = data.pointer("/distribution/target").and_then(Value::as_str);
    let expected_version = expected_version.to_string();
    if version != Some(expected_version.as_str())
        || official != Some(true)
        || target != Some(expected_target.triple())
    {
        return Err(UpdateError::VerificationFailed(
            "staged executable identity does not match the selected release".into(),
        ));
    }
    Ok(())
}
