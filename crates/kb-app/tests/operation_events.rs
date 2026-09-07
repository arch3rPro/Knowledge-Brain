use std::fs;

use kb_app::{UserPaths, apply_operation, create_adoption_plan, operation_events};
use kb_core::{ErrorCode, OperationEventKind};

#[test]
fn legacy_operation_returns_a_synthetic_event_without_writing_a_log() {
    let temporary = tempfile::tempdir().unwrap();
    let vault = temporary.path().join("vault");
    fs::create_dir(&vault).unwrap();
    fs::write(vault.join("notes.md"), "Human note").unwrap();
    let paths = UserPaths::new(
        temporary.path().join("config"),
        temporary.path().join("state"),
        temporary.path().join("cache"),
    );
    let plan = create_adoption_plan(&vault, &paths).unwrap();
    let events_path = paths
        .state_dir
        .join("operations")
        .join(plan.operation_id.to_string())
        .join("events.json");
    fs::remove_file(&events_path).unwrap();
    assert!(!events_path.exists());

    let report = operation_events(&paths, plan.operation_id).unwrap();

    assert_eq!(report.events.len(), 1);
    assert_eq!(report.events[0].kind, OperationEventKind::Planned);
    assert!(!report.terminal);
    assert!(!events_path.exists());
}

#[test]
fn corrupt_persisted_event_log_is_not_served() {
    let temporary = tempfile::tempdir().unwrap();
    let vault = temporary.path().join("vault");
    fs::create_dir(&vault).unwrap();
    fs::write(vault.join("notes.md"), "Human note").unwrap();
    let paths = UserPaths::new(
        temporary.path().join("config"),
        temporary.path().join("state"),
        temporary.path().join("cache"),
    );
    let plan = create_adoption_plan(&vault, &paths).unwrap();
    let events_path = paths
        .state_dir
        .join("operations")
        .join(plan.operation_id.to_string())
        .join("events.json");
    fs::write(
        events_path,
        format!(
            "{{\"schema_version\":\"v1.0\",\"operation_id\":\"{}\",\"events\":[]}}",
            plan.operation_id
        ),
    )
    .unwrap();

    assert!(operation_events(&paths, plan.operation_id).is_err());
}

#[test]
fn adoption_records_ordered_progress_and_completed_retry_is_idempotent() {
    let temporary = tempfile::tempdir().unwrap();
    let vault = temporary.path().join("vault");
    fs::create_dir(&vault).unwrap();
    fs::write(vault.join("notes.md"), "Human note").unwrap();
    let paths = UserPaths::new(
        temporary.path().join("config"),
        temporary.path().join("state"),
        temporary.path().join("cache"),
    );
    let plan = create_adoption_plan(&vault, &paths).unwrap();

    apply_operation(&paths, plan.operation_id).unwrap();
    let first = operation_events(&paths, plan.operation_id).unwrap();
    apply_operation(&paths, plan.operation_id).unwrap();
    let retried = operation_events(&paths, plan.operation_id).unwrap();

    assert_eq!(first, retried);
    assert!(first.terminal);
    assert_eq!(
        first.events.first().unwrap().kind,
        OperationEventKind::Planned
    );
    assert_eq!(
        first.events.last().unwrap().kind,
        OperationEventKind::Applied
    );
    assert!(
        first
            .events
            .iter()
            .any(|event| event.kind == OperationEventKind::Progress)
    );
    assert!(
        first
            .events
            .iter()
            .enumerate()
            .all(|(index, event)| event.id == index as u64 + 1)
    );
}

#[test]
fn failed_adoption_records_a_terminal_attempt_event() {
    let temporary = tempfile::tempdir().unwrap();
    let vault = temporary.path().join("vault");
    fs::create_dir(&vault).unwrap();
    fs::write(vault.join("notes.md"), "Before").unwrap();
    let paths = UserPaths::new(
        temporary.path().join("config"),
        temporary.path().join("state"),
        temporary.path().join("cache"),
    );
    let plan = create_adoption_plan(&vault, &paths).unwrap();
    fs::write(vault.join("notes.md"), "After").unwrap();

    let error = apply_operation(&paths, plan.operation_id).unwrap_err();
    let report = operation_events(&paths, plan.operation_id).unwrap();

    assert_eq!(error.code, ErrorCode::PlanStale);
    assert_eq!(
        report.events.last().unwrap().kind,
        OperationEventKind::Failed
    );
    assert!(report.terminal);
}
