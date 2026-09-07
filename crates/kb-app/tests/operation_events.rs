use std::fs;

use kb_app::{UserPaths, create_adoption_plan, operation_events};
use kb_core::OperationEventKind;

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
