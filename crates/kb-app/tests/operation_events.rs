use std::{collections::BTreeMap, fs};

use kb_app::{
    AppContext, AppRequest, InitRequest, OperationRequest, UserPaths, apply_operation,
    create_adoption_plan, operation_events, run,
};
use kb_core::{
    CURRENT_SCHEMA_VERSION, ErrorCode, KnowledgeChangeRequest, KnowledgePlanRequest,
    OperationEvent, OperationEventKind, OperationEventLog, PortableRelativePath,
};

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

#[test]
fn application_event_request_is_fixed_to_the_selected_vault() {
    let temporary = tempfile::tempdir().unwrap();
    let context = application_context(temporary.path());
    let first = temporary.path().join("first");
    let second = temporary.path().join("second");
    for vault in [&first, &second] {
        run(
            AppRequest::Init(InitRequest {
                target: vault.clone(),
            }),
            &context,
        )
        .unwrap();
    }
    let plan = run(
        AppRequest::PlanCreate {
            vault: Some(second.display().to_string()),
            request: knowledge_request(),
        },
        &context,
    )
    .unwrap();
    let operation_id = plan["operation_id"].as_str().unwrap().parse().unwrap();

    let denied = run(
        AppRequest::Operation(OperationRequest::EventsForVault {
            vault: first.display().to_string(),
            operation_id,
        }),
        &context,
    )
    .unwrap_err();
    assert_eq!(denied.code, ErrorCode::AuthDenied);

    let report = run(
        AppRequest::Operation(OperationRequest::EventsForVault {
            vault: second.display().to_string(),
            operation_id,
        }),
        &context,
    )
    .unwrap();
    assert_eq!(report["operation_id"], operation_id.to_string());
    assert_eq!(report["terminal"], false);
    assert_eq!(report["events"][0]["kind"], "planned");

    let events_path = temporary
        .path()
        .join("state/operations")
        .join(operation_id.to_string())
        .join("events.json");
    let mut log: OperationEventLog =
        serde_json::from_slice(&fs::read(&events_path).unwrap()).unwrap();
    log.events.push(OperationEvent {
        id: 2,
        kind: OperationEventKind::Applying,
        completed: Some(0),
        total: Some(3),
        message: "Knowledge apply started.".into(),
        recorded_at: "2026-09-07T03:01:00Z".into(),
    });
    fs::write(&events_path, serde_json::to_vec_pretty(&log).unwrap()).unwrap();
    let active = run(
        AppRequest::Operation(OperationRequest::EventsForVault {
            vault: second.display().to_string(),
            operation_id,
        }),
        &context,
    )
    .unwrap();
    assert_eq!(active["terminal"], false);
    assert_eq!(active["events"][1]["kind"], "applying");

    run(
        AppRequest::ApplyForVault {
            vault: second.display().to_string(),
            operation_id,
        },
        &context,
    )
    .unwrap();
    let completed = run(
        AppRequest::Operation(OperationRequest::EventsForVault {
            vault: second.display().to_string(),
            operation_id,
        }),
        &context,
    )
    .unwrap();
    assert_eq!(completed["terminal"], true);
    assert_eq!(
        completed["events"].as_array().unwrap().last().unwrap()["kind"],
        "applied"
    );
}

fn application_context(base: &std::path::Path) -> AppContext {
    let environment = BTreeMap::from([
        (
            "KB_CONFIG_DIR".to_owned(),
            base.join("config").display().to_string(),
        ),
        (
            "KB_STATE_DIR".to_owned(),
            base.join("state").display().to_string(),
        ),
        (
            "KB_CACHE_DIR".to_owned(),
            base.join("cache").display().to_string(),
        ),
    ]);
    AppContext::new(environment, base.to_path_buf())
}

fn knowledge_request() -> KnowledgePlanRequest {
    KnowledgePlanRequest {
        schema_version: CURRENT_SCHEMA_VERSION,
        changes: vec![KnowledgeChangeRequest {
            path: PortableRelativePath::parse("articles/events.md").unwrap(),
            before_sha256: None,
            summary: "Add operation event notes.".into(),
            content: "---\ntype: Article\ntitle: Events\nstatus: stable\ngenerated:\n  by: process:test\n  at: 2026-09-07T03:00:00Z\nsources:\n  - id: source\n    resource: https://example.com\nkb:\n  managed: true\n---\n\n# Events\n"
                .into(),
        }],
    }
}
