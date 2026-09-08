use ureq::ResponseExt;

use crate::UpdateError;

/// The narrow network boundary used by release lookup and download verification.
pub trait ReleaseTransport {
    /// Resolves an HTTPS URL and returns the final URL after redirects.
    ///
    /// # Errors
    ///
    /// Returns [`UpdateError::Transport`] when the request fails or resolves to
    /// a non-HTTPS URL.
    fn resolve_url(&self, url: &str) -> Result<String, UpdateError>;

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
    fn resolve_url(&self, url: &str) -> Result<String, UpdateError> {
        require_https(url)?;
        let response = ureq::get(url)
            .header("User-Agent", "knowledge-brain-updater")
            .call()
            .map_err(|error| UpdateError::Transport(format!("resolve {url}: {error}")))?;
        let resolved = response.get_uri().to_string();
        require_https(&resolved)?;
        Ok(resolved)
    }

    fn get_bytes(&self, url: &str) -> Result<Vec<u8>, UpdateError> {
        require_https(url)?;
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

fn require_https(url: &str) -> Result<(), UpdateError> {
    if url.starts_with("https://") {
        Ok(())
    } else {
        Err(UpdateError::Transport(
            "release downloads must use HTTPS".into(),
        ))
    }
}
