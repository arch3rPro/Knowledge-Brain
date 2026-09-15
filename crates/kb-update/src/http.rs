use std::io;
use std::thread;
use std::time::{Duration, SystemTime};

use ureq::ResponseExt;

use crate::UpdateError;

const USER_AGENT: &str = "knowledge-brain-updater";
const DOWNLOAD_LIMIT: u64 = 128 * 1024 * 1024;

/// The release request stage that failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportStage {
    Resolve,
    Download,
}

impl std::fmt::Display for TransportStage {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Resolve => "resolve",
            Self::Download => "download",
        })
    }
}

/// A bounded retry policy for release metadata and asset requests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryPolicy {
    max_attempts: usize,
    max_total_wait: Duration,
    backoff: [Duration; 2],
}

impl RetryPolicy {
    #[must_use]
    pub const fn max_attempts(self) -> usize {
        self.max_attempts
    }

    #[must_use]
    pub const fn max_total_wait(self) -> Duration {
        self.max_total_wait
    }

    #[must_use]
    pub const fn backoff(self) -> [Duration; 2] {
        self.backoff
    }
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            max_total_wait: Duration::from_secs(3),
            backoff: [Duration::from_secs(1), Duration::from_secs(2)],
        }
    }
}

/// Safe, structured context for a failed release request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransportFailure {
    stage: TransportStage,
    host: String,
    attempts: usize,
    retryable: bool,
    cause: String,
}

impl TransportFailure {
    #[must_use]
    pub fn for_url(
        stage: TransportStage,
        url: &str,
        attempts: usize,
        retryable: bool,
        cause: &str,
    ) -> Self {
        Self {
            stage,
            host: safe_endpoint(url),
            attempts,
            retryable,
            cause: redact_cause(cause),
        }
    }

    #[must_use]
    pub const fn stage(&self) -> TransportStage {
        self.stage
    }

    #[must_use]
    pub fn host(&self) -> &str {
        &self.host
    }

    #[must_use]
    pub const fn attempts(&self) -> usize {
        self.attempts
    }

    #[must_use]
    pub const fn retryable(&self) -> bool {
        self.retryable
    }

    #[must_use]
    pub fn cause(&self) -> &str {
        &self.cause
    }
}

impl std::fmt::Display for TransportFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let attempts = if self.attempts == 1 {
            "1 attempt".to_owned()
        } else {
            format!("{} attempts", self.attempts)
        };
        write!(
            formatter,
            "{} {} failed after {attempts}: {}",
            self.stage, self.host, self.cause
        )
    }
}

/// The narrow network boundary used by release lookup and download verification.
pub trait ReleaseTransport {
    /// Resolves an HTTPS URL and returns the final URL after redirects.
    ///
    /// # Errors
    ///
    /// Returns a transport error when the request fails or resolves to
    /// a non-HTTPS URL.
    fn resolve_url(&self, url: &str) -> Result<String, UpdateError>;

    /// Fetches an opaque release asset from a release URL.
    ///
    /// # Errors
    ///
    /// Returns a transport error when the request fails or exceeds the
    /// download limit.
    fn get_bytes(&self, url: &str) -> Result<Vec<u8>, UpdateError>;
}

/// The production HTTPS transport for GitHub Release metadata and assets.
#[derive(Debug, Default)]
pub struct UreqTransport;

impl ReleaseTransport for UreqTransport {
    fn resolve_url(&self, url: &str) -> Result<String, UpdateError> {
        require_https(url, TransportStage::Resolve)?;
        retry(
            RetryPolicy::default(),
            TransportStage::Resolve,
            url,
            || {
                let response = request(url)?;
                let resolved = response.get_uri().to_string();
                require_https_attempt(&resolved)?;
                Ok(resolved)
            },
            thread::sleep,
        )
    }

    fn get_bytes(&self, url: &str) -> Result<Vec<u8>, UpdateError> {
        require_https(url, TransportStage::Download)?;
        retry(
            RetryPolicy::default(),
            TransportStage::Download,
            url,
            || {
                let mut response = request(url)?;
                response
                    .body_mut()
                    .with_config()
                    .limit(DOWNLOAD_LIMIT)
                    .read_to_vec()
                    .map_err(AttemptFailure::from_ureq)
            },
            thread::sleep,
        )
    }
}

fn request(url: &str) -> Result<ureq::http::Response<ureq::Body>, AttemptFailure> {
    let response = ureq::get(url)
        .header("User-Agent", USER_AGENT)
        .config()
        .http_status_as_error(false)
        .https_only(true)
        .timeout_global(Some(Duration::from_secs(60)))
        .build()
        .call()
        .map_err(AttemptFailure::from_ureq)?;
    let status = response.status().as_u16();
    if status >= 400 {
        let retry_after = response
            .headers()
            .get("retry-after")
            .and_then(|value| value.to_str().ok())
            .and_then(parse_retry_after);
        return Err(AttemptFailure::http_status(status, retry_after));
    }
    Ok(response)
}

fn parse_retry_after(value: &str) -> Option<Duration> {
    if let Ok(seconds) = value.trim().parse::<u64>() {
        return Some(Duration::from_secs(seconds));
    }
    let retry_at = httpdate::parse_http_date(value).ok()?;
    retry_at.duration_since(SystemTime::now()).ok()
}

fn retry<T, Operation, Wait>(
    policy: RetryPolicy,
    stage: TransportStage,
    url: &str,
    mut operation: Operation,
    mut wait: Wait,
) -> Result<T, UpdateError>
where
    Operation: FnMut() -> Result<T, AttemptFailure>,
    Wait: FnMut(Duration),
{
    let mut waited = Duration::ZERO;
    for attempt in 1..=policy.max_attempts {
        match operation() {
            Ok(value) => return Ok(value),
            Err(failure) => {
                let can_retry = failure.retryable && attempt < policy.max_attempts;
                if !can_retry {
                    return Err(transport_error(stage, url, attempt, &failure));
                }
                let delay = failure
                    .retry_after
                    .unwrap_or(policy.backoff[attempt.saturating_sub(1).min(1)]);
                let Some(next_waited) = waited.checked_add(delay) else {
                    return Err(transport_error(stage, url, attempt, &failure));
                };
                if next_waited > policy.max_total_wait {
                    return Err(transport_error(stage, url, attempt, &failure));
                }
                wait(delay);
                waited = next_waited;
            }
        }
    }
    unreachable!("retry loop always returns")
}

fn transport_error(
    stage: TransportStage,
    url: &str,
    attempt: usize,
    failure: &AttemptFailure,
) -> UpdateError {
    UpdateError::Transport(TransportFailure::for_url(
        stage,
        url,
        attempt,
        failure.retryable,
        &failure.cause,
    ))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttemptFailure {
    retryable: bool,
    cause: String,
    retry_after: Option<Duration>,
}

impl AttemptFailure {
    #[must_use]
    pub fn connection(_detail: &str) -> Self {
        Self::retryable("connection interrupted")
    }

    #[must_use]
    pub fn timeout(_detail: &str) -> Self {
        Self::retryable("request timed out")
    }

    #[must_use]
    pub fn premature_close(_detail: &str) -> Self {
        Self::retryable("response closed before completion")
    }

    #[must_use]
    pub fn certificate(_detail: &str) -> Self {
        Self::permanent("TLS certificate validation failed")
    }

    #[must_use]
    pub fn invalid_redirect(_detail: &str) -> Self {
        Self::permanent("redirect target is not an allowed HTTPS URL")
    }

    #[must_use]
    pub fn http_status(status: u16, retry_after: Option<Duration>) -> Self {
        Self {
            retryable: matches!(status, 408 | 429 | 500 | 502 | 503 | 504),
            cause: format!("HTTP status {status}"),
            retry_after,
        }
    }

    fn retryable(cause: &str) -> Self {
        Self {
            retryable: true,
            cause: cause.to_owned(),
            retry_after: None,
        }
    }

    fn permanent(cause: &str) -> Self {
        Self {
            retryable: false,
            cause: cause.to_owned(),
            retry_after: None,
        }
    }

    fn from_body_error(error: &io::Error) -> Self {
        match error.kind() {
            io::ErrorKind::UnexpectedEof => Self::premature_close(""),
            io::ErrorKind::TimedOut => Self::timeout(""),
            io::ErrorKind::ConnectionReset
            | io::ErrorKind::ConnectionAborted
            | io::ErrorKind::NotConnected
            | io::ErrorKind::BrokenPipe
            | io::ErrorKind::Interrupted
            | io::ErrorKind::WouldBlock => Self::connection(""),
            _ => Self::permanent("response body could not be read"),
        }
    }

    fn from_ureq(error: ureq::Error) -> Self {
        match error {
            ureq::Error::StatusCode(status) => Self::http_status(status, None),
            ureq::Error::Timeout(_) => Self::timeout(""),
            ureq::Error::HostNotFound | ureq::Error::ConnectionFailed => Self::connection(""),
            ureq::Error::Io(error) => Self::from_body_error(&error),
            ureq::Error::ConnectProxyFailed(_) => Self::retryable("proxy connection failed"),
            ureq::Error::Tls(_)
            | ureq::Error::Rustls(_)
            | ureq::Error::Pem(_)
            | ureq::Error::InvalidProxyUrl
            | ureq::Error::RequireHttpsOnly(_)
            | ureq::Error::TlsRequired => Self::permanent("TLS or proxy configuration failed"),
            _ => Self::permanent("release request failed"),
        }
    }
}

fn require_https(url: &str, stage: TransportStage) -> Result<(), UpdateError> {
    require_https_attempt(url).map_err(|failure| transport_error(stage, url, 1, &failure))
}

fn require_https_attempt(url: &str) -> Result<(), AttemptFailure> {
    let uri = url
        .parse::<ureq::http::Uri>()
        .map_err(|_| AttemptFailure::invalid_redirect(""))?;
    if uri.scheme_str() == Some("https") && uri.host().is_some() {
        Ok(())
    } else {
        Err(AttemptFailure::invalid_redirect(""))
    }
}

fn safe_endpoint(url: &str) -> String {
    url.parse::<ureq::http::Uri>()
        .ok()
        .and_then(|uri| {
            let scheme = uri.scheme_str()?.to_owned();
            let host = uri.host()?.to_owned();
            Some(format!("{scheme}://{host}"))
        })
        .unwrap_or_else(|| "release service".to_owned())
}

fn redact_cause(cause: &str) -> String {
    cause
        .split_whitespace()
        .map(|part| {
            if part.contains("://") {
                "[endpoint]"
            } else {
                part
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[doc(hidden)]
pub mod retry_test_support {
    use std::time::Duration;

    pub use super::AttemptFailure;
    use super::{RetryPolicy, TransportStage, UpdateError, retry};

    pub fn run_scripted<T, Operation, Wait>(
        policy: RetryPolicy,
        stage: TransportStage,
        url: &str,
        operation: Operation,
        wait: Wait,
    ) -> Result<T, UpdateError>
    where
        Operation: FnMut() -> Result<T, AttemptFailure>,
        Wait: FnMut(Duration),
    {
        retry(policy, stage, url, operation, wait)
    }
}
