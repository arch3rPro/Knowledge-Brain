use kb_core::{
    CURRENT_SCHEMA_VERSION, OperationEvent, OperationEventKind, OperationEventLog, OperationId,
};

fn event(
    id: u64,
    kind: OperationEventKind,
    completed: Option<u64>,
    total: Option<u64>,
) -> OperationEvent {
    OperationEvent {
        id,
        kind,
        completed,
        total,
        message: "Operation state changed.".into(),
        recorded_at: "2026-09-07T03:00:00Z".into(),
    }
}

#[test]
fn event_log_accepts_ordered_bounded_progress() {
    let operation_id = OperationId::new();
    let log = OperationEventLog {
        schema_version: CURRENT_SCHEMA_VERSION,
        operation_id,
        events: vec![
            event(1, OperationEventKind::Planned, Some(0), Some(2)),
            event(2, OperationEventKind::Applying, Some(0), Some(2)),
            event(3, OperationEventKind::Progress, Some(1), Some(2)),
            event(4, OperationEventKind::Applied, Some(2), Some(2)),
        ],
    };

    log.validate(operation_id, 8).unwrap();
    assert!(log.is_terminal());
}

#[test]
fn event_log_rejects_invalid_identity_order_progress_and_text() {
    let operation_id = OperationId::new();
    let other = OperationId::new();
    let cases = [
        OperationEventLog {
            schema_version: CURRENT_SCHEMA_VERSION,
            operation_id: other,
            events: vec![event(1, OperationEventKind::Planned, None, None)],
        },
        OperationEventLog {
            schema_version: CURRENT_SCHEMA_VERSION,
            operation_id,
            events: vec![event(2, OperationEventKind::Planned, None, None)],
        },
        OperationEventLog {
            schema_version: CURRENT_SCHEMA_VERSION,
            operation_id,
            events: vec![event(1, OperationEventKind::Progress, Some(3), Some(2))],
        },
        OperationEventLog {
            schema_version: CURRENT_SCHEMA_VERSION,
            operation_id,
            events: vec![OperationEvent {
                message: "two\nlines".into(),
                ..event(1, OperationEventKind::Failed, None, None)
            }],
        },
    ];

    for log in cases {
        assert!(log.validate(operation_id, 8).is_err());
    }
}
