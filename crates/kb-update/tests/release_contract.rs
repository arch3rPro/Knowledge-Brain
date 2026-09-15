use std::time::Duration;

use kb_update::{RetryPolicy, TransportFailure, TransportStage, UpdateError};

#[test]
fn retry_defaults_are_bounded_to_three_total_attempts() {
    let policy = RetryPolicy::default();

    assert_eq!(policy.max_attempts(), 3);
    assert_eq!(
        policy.backoff(),
        [Duration::from_secs(1), Duration::from_secs(2)]
    );
    assert_eq!(policy.max_total_wait(), Duration::from_secs(3));
}

#[test]
fn transport_diagnostics_do_not_expose_url_secrets() {
    let failure = TransportFailure::for_url(
        TransportStage::Download,
        "https://alice:secret@example.com/releases/file?token=private",
        3,
        true,
        "timed out via https://proxy-user:proxy-pass@proxy.example",
    );
    let rendered = UpdateError::Transport(failure).to_string();

    assert!(rendered.contains("download"));
    assert!(rendered.contains("example.com"));
    assert!(rendered.contains("3 attempts"));
    assert!(!rendered.contains("alice"));
    assert!(!rendered.contains("secret"));
    assert!(!rendered.contains("token"));
    assert!(!rendered.contains("proxy-user"));
    assert!(!rendered.contains("proxy-pass"));
}

#[test]
fn certificate_failures_are_not_marked_retryable() {
    let failure = TransportFailure::for_url(
        TransportStage::Resolve,
        "https://github.com/example",
        1,
        false,
        "certificate validation failed",
    );

    assert!(!failure.retryable());
    assert_eq!(failure.attempts(), 1);
}
