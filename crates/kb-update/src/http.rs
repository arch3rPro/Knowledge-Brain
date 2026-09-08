use serde_json::Value;

use crate::UpdateError;

/// The narrow network boundary used by release lookup and download verification.
pub trait ReleaseTransport {
    /// Fetches a JSON document from a release URL.
    ///
    /// # Errors
    ///
    /// Returns [`UpdateError::Transport`] when the request or JSON parsing fails.
    fn get_json(&self, url: &str) -> Result<Value, UpdateError>;

    /// Fetches an opaque release asset from a release URL.
    ///
    /// # Errors
    ///
    /// Returns [`UpdateError::Transport`] when the request fails or exceeds the
    /// download limit.
    fn get_bytes(&self, url: &str) -> Result<Vec<u8>, UpdateError>;
}

/// The production HTTPS transport for GitHub Release metadata and assets.
#[derive(Debug, Default)]
pub struct UreqTransport;

impl ReleaseTransport for UreqTransport {
    fn get_json(&self, url: &str) -> Result<Value, UpdateError> {
        let bytes = self.get_bytes(url)?;
        serde_json::from_slice(&bytes)
            .map_err(|error| UpdateError::Transport(format!("parse release JSON: {error}")))
    }

    fn get_bytes(&self, url: &str) -> Result<Vec<u8>, UpdateError> {
        if !url.starts_with("https://") {
            return Err(UpdateError::Transport(
                "release downloads must use HTTPS".into(),
            ));
        }
        ureq::get(url)
            .header("User-Agent", "knowledge-brain-updater")
            .call()
            .map_err(|error| UpdateError::Transport(format!("download {url}: {error}")))?
            .body_mut()
            .with_config()
            .limit(128 * 1024 * 1024)
            .read_to_vec()
            .map_err(|error| UpdateError::Transport(format!("read {url}: {error}")))
    }
}
