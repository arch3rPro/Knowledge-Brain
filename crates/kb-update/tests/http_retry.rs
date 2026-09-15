use kb_update::retry_test_support::{AttemptFailure, run_scripted};
use kb_update::{RetryPolicy, TransportStage};

#[test]
fn transient_connection_failure_then_success_retries_once() {
    let mut attempts = 0;
    let mut waits = Vec::new();
    let value = run_scripted(
        RetryPolicy::default(),
        TransportStage::Download,
        "https://example.com/asset?token=secret",
        || {
            attempts += 1;
            if attempts == 1 {
                Err(AttemptFailure::connection("connection reset"))
            } else {
                Ok("downloaded")
            }
        },
        |delay| waits.push(delay),
    )
    .unwrap();

    assert_eq!(value, "downloaded");
    assert_eq!(attempts, 2);
    assert_eq!(waits, [std::time::Duration::from_secs(1)]);
}

#[test]
fn timeout_exhausts_at_exactly_three_calls() {
    let mut attempts = 0;
    let error = run_scripted::<(), _, _>(
        RetryPolicy::default(),
        TransportStage::Resolve,
        "https://example.com/latest",
        || {
            attempts += 1;
            Err(AttemptFailure::timeout("request timed out"))
        },
        |_| {},
    )
    .unwrap_err();

    assert_eq!(attempts, 3);
    let failure = error.transport_failure().unwrap();
    assert_eq!(failure.attempts(), 3);
    assert!(failure.retryable());
}

#[test]
fn premature_close_and_retryable_statuses_are_retried() {
    for failure in [
        AttemptFailure::premature_close("peer disconnected"),
        AttemptFailure::http_status(408, None),
        AttemptFailure::http_status(429, None),
        AttemptFailure::http_status(500, None),
        AttemptFailure::http_status(502, None),
        AttemptFailure::http_status(503, None),
        AttemptFailure::http_status(504, None),
    ] {
        let mut attempts = 0;
        let result = run_scripted(
            RetryPolicy::default(),
            TransportStage::Download,
            "https://example.com/asset",
            || {
                attempts += 1;
                if attempts == 1 {
                    Err(failure.clone())
                } else {
                    Ok(())
                }
            },
            |_| {},
        );
        assert!(result.is_ok(), "{failure:?}");
        assert_eq!(attempts, 2);
    }
}

#[test]
fn retry_after_is_bounded_by_total_wait_budget() {
    let mut waits = Vec::new();
    let mut attempts = 0;
    let error = run_scripted::<(), _, _>(
        RetryPolicy::default(),
        TransportStage::Download,
        "https://example.com/asset",
        || {
            attempts += 1;
            Err(AttemptFailure::http_status(
                429,
                Some(std::time::Duration::from_secs(60)),
            ))
        },
        |delay| waits.push(delay),
    )
    .unwrap_err();

    assert_eq!(attempts, 1);
    assert!(waits.is_empty());
    assert!(error.transport_failure().unwrap().retryable());
}

#[test]
fn retry_after_within_budget_overrides_backoff() {
    let mut waits = Vec::new();
    let mut attempts = 0;
    let result = run_scripted(
        RetryPolicy::default(),
        TransportStage::Download,
        "https://example.com/asset",
        || {
            attempts += 1;
            if attempts == 1 {
                Err(AttemptFailure::http_status(
                    503,
                    Some(std::time::Duration::from_secs(2)),
                ))
            } else {
                Ok(())
            }
        },
        |delay| waits.push(delay),
    );

    assert!(result.is_ok());
    assert_eq!(waits, [std::time::Duration::from_secs(2)]);
}

#[test]
fn permanent_failures_are_not_retried() {
    for failure in [
        AttemptFailure::certificate("unknown issuer"),
        AttemptFailure::http_status(404, None),
        AttemptFailure::invalid_redirect("redirect must use HTTPS"),
    ] {
        let mut attempts = 0;
        let error = run_scripted::<(), _, _>(
            RetryPolicy::default(),
            TransportStage::Resolve,
            "https://example.com/latest",
            || {
                attempts += 1;
                Err(failure.clone())
            },
            |_| {},
        )
        .unwrap_err();
        assert_eq!(attempts, 1);
        assert!(!error.transport_failure().unwrap().retryable());
    }
}
