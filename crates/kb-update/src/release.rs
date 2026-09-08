use semver::Version;
use serde_json::Value;
use thiserror::Error;

use crate::{BuildIdentity, ReleaseTarget, ReleaseTransport};

/// GitHub's public redirect to the repository's latest stable release.
pub const RELEASES_LATEST_URL: &str =
    "https://github.com/arch3rPro/Knowledge-Brain/releases/latest";

const RELEASE_TAG_PREFIX: &str = "https://github.com/arch3rPro/Knowledge-Brain/releases/tag/v";
const RELEASE_DOWNLOAD_PREFIX: &str =
    "https://github.com/arch3rPro/Knowledge-Brain/releases/download/v";

/// A newer official release and its three required download locations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AvailableRelease {
    pub version: Version,
    pub target: ReleaseTarget,
    pub archive_url: String,
    pub checksums_url: String,
    pub signature_url: String,
}

/// The explicit update status for one official executable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateCheck {
    pub current: Version,
    pub latest: Option<AvailableRelease>,
    pub update_available: bool,
}

/// Parses an official stable GitHub Release response for `identity`.
///
/// # Errors
///
/// Returns [`UpdateError::InvalidRelease`] for malformed, draft, prerelease, or
/// unsupported-installation responses, and [`UpdateError::MissingAsset`] when a
/// newer release lacks a required target-specific asset.
pub fn parse_latest_release(
    identity: &BuildIdentity,
    response: &Value,
) -> Result<UpdateCheck, UpdateError> {
    let current = identity.version().clone();
    let target = identity.target().ok_or_else(|| {
        UpdateError::InvalidRelease("this installation is not an official release".into())
    })?;
    if !identity.can_update() {
        return Err(UpdateError::InvalidRelease(
            "this installation has no embedded update verification key".into(),
        ));
    }
    if response.get("prerelease").and_then(Value::as_bool) != Some(false)
        || response.get("draft").and_then(Value::as_bool) != Some(false)
    {
        return Err(UpdateError::InvalidRelease(
            "latest release must be a published stable release".into(),
        ));
    }
    let tag = required_string(response, "tag_name")?;
    let version = Version::parse(
        tag.strip_prefix('v')
            .ok_or_else(|| UpdateError::InvalidRelease("release tag must start with v".into()))?,
    )
    .map_err(|error| UpdateError::InvalidRelease(format!("invalid release tag {tag}: {error}")))?;
    if !version.pre.is_empty() {
        return Err(UpdateError::InvalidRelease(
            "latest release must not be a prerelease".into(),
        ));
    }
    if version <= current {
        return Ok(UpdateCheck {
            current,
            latest: None,
            update_available: false,
        });
    }

    let assets = response
        .get("assets")
        .and_then(Value::as_array)
        .ok_or_else(|| UpdateError::InvalidRelease("release assets must be an array".into()))?;
    let archive_name = target.asset_name(&version);
    let archive_url = asset_url(assets, &archive_name, target)?;
    let checksums_url = asset_url(assets, "SHA256SUMS", target)?;
    let signature_url = asset_url(assets, "SHA256SUMS.minisig", target)?;
    Ok(UpdateCheck {
        current,
        latest: Some(AvailableRelease {
            version,
            target,
            archive_url,
            checksums_url,
            signature_url,
        }),
        update_available: true,
    })
}

/// Fetches and parses the latest official stable release for `identity`.
///
/// # Errors
///
/// Returns [`UpdateError::Transport`] for a network failure and propagates
/// malformed-release errors from [`parse_latest_release`].
pub fn check_for_update(
    identity: &BuildIdentity,
    transport: &dyn ReleaseTransport,
) -> Result<UpdateCheck, UpdateError> {
    let resolved_url = transport.resolve_url(RELEASES_LATEST_URL)?;
    parse_latest_release_url(identity, &resolved_url)
}

fn parse_latest_release_url(
    identity: &BuildIdentity,
    resolved_url: &str,
) -> Result<UpdateCheck, UpdateError> {
    let current = identity.version().clone();
    let target = identity.target().ok_or_else(|| {
        UpdateError::InvalidRelease("this installation is not an official release".into())
    })?;
    if !identity.can_update() {
        return Err(UpdateError::InvalidRelease(
            "this installation has no embedded update verification key".into(),
        ));
    }

    let version_text = resolved_url
        .strip_prefix(RELEASE_TAG_PREFIX)
        .ok_or_else(|| {
            UpdateError::InvalidRelease(format!(
                "latest release resolved to an unexpected URL: {resolved_url}"
            ))
        })?;
    let version = Version::parse(version_text).map_err(|error| {
        UpdateError::InvalidRelease(format!("invalid release tag v{version_text}: {error}"))
    })?;
    if !version.pre.is_empty() {
        return Err(UpdateError::InvalidRelease(
            "latest release must not be a prerelease".into(),
        ));
    }
    if version <= current {
        return Ok(UpdateCheck {
            current,
            latest: None,
            update_available: false,
        });
    }

    let base_url = format!("{RELEASE_DOWNLOAD_PREFIX}{version}");
    Ok(UpdateCheck {
        current,
        latest: Some(AvailableRelease {
            archive_url: format!("{base_url}/{}", target.asset_name(&version)),
            checksums_url: format!("{base_url}/SHA256SUMS"),
            signature_url: format!("{base_url}/SHA256SUMS.minisig"),
            version,
            target,
        }),
        update_available: true,
    })
}

fn required_string<'a>(value: &'a Value, field: &str) -> Result<&'a str, UpdateError> {
    value
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| UpdateError::InvalidRelease(format!("release {field} must be a string")))
}

fn asset_url(
    assets: &[Value],
    expected_name: &str,
    target: ReleaseTarget,
) -> Result<String, UpdateError> {
    let matching = assets
        .iter()
        .filter(|asset| asset.get("name").and_then(Value::as_str) == Some(expected_name))
        .collect::<Vec<_>>();
    if matching.len() != 1 {
        return Err(UpdateError::MissingAsset {
            target,
            name: expected_name.to_owned(),
        });
    }
    let url = required_string(matching[0], "browser_download_url")?;
    if !url.starts_with("https://") {
        return Err(UpdateError::InvalidRelease(format!(
            "asset {expected_name} must use HTTPS"
        )));
    }
    Ok(url.to_owned())
}

/// A release lookup or verification error without vault-specific semantics.
#[derive(Debug, Error)]
pub enum UpdateError {
    #[error("invalid release metadata: {0}")]
    InvalidRelease(String),
    #[error("release is missing required asset {name} for {target:?}")]
    MissingAsset { target: ReleaseTarget, name: String },
    #[error("release transport failed: {0}")]
    Transport(String),
    #[error("release verification failed: {0}")]
    VerificationFailed(String),
    #[error("executable replacement failed: {0}")]
    ReplacementFailed(String),
}
