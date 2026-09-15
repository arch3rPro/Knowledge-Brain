use std::fs;

use kb_app::{UpdateStore, UserPaths};
use kb_core::{
    OperationId, UpdateComponent, UpdateComponentKind, UpdateComponentState,
    UpdateConfirmationToken, UpdateExecutionState, UpdatePhase, UpdatePlan, UpdatePlanState,
    UpdateScope, UpdateScopeMode,
};
use time::{Duration, OffsetDateTime, format_description::well_known::Rfc3339};

fn paths(temp: &tempfile::TempDir) -> UserPaths {
    UserPaths::new(
        temp.path().join("config"),
        temp.path().join("state"),
        temp.path().join("cache"),
    )
}

fn plan(expires_at: OffsetDateTime) -> UpdatePlan {
    UpdatePlan {
        kind: "update".into(),
        phase: UpdatePhase::Preview,
        state: UpdatePlanState::ReviewRequired,
        operation_id: OperationId::new(),
        current_version: "0.1.3".into(),
        target_version: "0.1.4".into(),
        scope: UpdateScope {
            mode: UpdateScopeMode::RegisteredVaults,
            vaults: Vec::new(),
        },
        excluded_vaults: Vec::new(),
        components: vec![component(UpdateComponentState::Pending)],
        conflicts: Vec::new(),
        skipped: Vec::new(),
        untouched: vec!["user notes".into()],
        created_at: OffsetDateTime::now_utc().format(&Rfc3339).unwrap(),
        expires_at: expires_at.format(&Rfc3339).unwrap(),
    }
}

fn component(state: UpdateComponentState) -> UpdateComponent {
    UpdateComponent {
        id: "executable".into(),
        kind: UpdateComponentKind::Executable,
        state,
        vault_id: None,
        host: None,
        skill_scope: None,
        from_version: Some("0.1.3".into()),
        to_version: Some("0.1.4".into()),
        changes: Vec::new(),
        message: "Update the executable.".into(),
    }
}

fn future_plan() -> UpdatePlan {
    plan(OffsetDateTime::now_utc() + Duration::hours(1))
}

#[test]
fn create_is_private_and_sets_atomic_latest_pointer() {
    let temp = tempfile::tempdir().unwrap();
    let paths = paths(&temp);
    let store = UpdateStore::new(&paths);
    let plan = future_plan();

    let operation = store.create(&plan).unwrap();

    assert_eq!(operation.execution_state, UpdateExecutionState::Preview);
    assert_eq!(store.latest().unwrap().unwrap(), operation);
    assert_eq!(
        fs::read_to_string(paths.state_dir.join("updates/latest")).unwrap(),
        plan.operation_id.to_string()
    );
    let directory = paths
        .state_dir
        .join("updates")
        .join(plan.operation_id.to_string());
    assert!(directory.join("plan.json").is_file());
    assert!(directory.join("operation.json").is_file());
    assert!(directory.join("effects.json").is_file());

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(directory).unwrap().permissions().mode() & 0o777,
            0o700
        );
    }
}

#[test]
fn confirm_rejects_wrong_expired_or_changed_plan_without_transitioning() {
    let temp = tempfile::tempdir().unwrap();
    let paths = paths(&temp);
    let store = UpdateStore::new(&paths);

    let valid = future_plan();
    store.create(&valid).unwrap();
    let wrong: UpdateConfirmationToken = "b".repeat(64).parse().unwrap();
    assert!(store.confirm(valid.operation_id, &wrong).is_err());
    assert_eq!(
        store.load(valid.operation_id).unwrap().execution_state,
        UpdateExecutionState::Preview
    );

    let mut expired = plan(OffsetDateTime::now_utc() - Duration::minutes(1));
    expired.created_at = (OffsetDateTime::now_utc() - Duration::hours(2))
        .format(&Rfc3339)
        .unwrap();
    let expired_token = expired.confirmation_token().unwrap();
    store.create(&expired).unwrap();
    assert!(store.confirm(expired.operation_id, &expired_token).is_err());

    let changed = future_plan();
    let changed_token = changed.confirmation_token().unwrap();
    store.create(&changed).unwrap();
    let plan_path = paths
        .state_dir
        .join("updates")
        .join(changed.operation_id.to_string())
        .join("plan.json");
    let mut bytes = fs::read(&plan_path).unwrap();
    bytes.push(b' ');
    fs::write(plan_path, bytes).unwrap();
    assert!(store.confirm(changed.operation_id, &changed_token).is_err());

    let altered_operation = future_plan();
    let altered_token = altered_operation.confirmation_token().unwrap();
    store.create(&altered_operation).unwrap();
    let operation_path = paths
        .state_dir
        .join("updates")
        .join(altered_operation.operation_id.to_string())
        .join("operation.json");
    let mut stored: serde_json::Value =
        serde_json::from_slice(&fs::read(&operation_path).unwrap()).unwrap();
    stored["target_version"] = serde_json::json!("9.9.9");
    fs::write(&operation_path, serde_json::to_vec_pretty(&stored).unwrap()).unwrap();
    assert!(
        store
            .confirm(altered_operation.operation_id, &altered_token)
            .is_err()
    );
}

#[test]
fn confirmation_binds_exact_plan_and_legal_transitions() {
    let temp = tempfile::tempdir().unwrap();
    let paths = paths(&temp);
    let store = UpdateStore::new(&paths);
    let plan = future_plan();
    let token = plan.confirmation_token().unwrap();
    store.create(&plan).unwrap();

    let confirmed = store.confirm(plan.operation_id, &token).unwrap();
    assert_eq!(confirmed.execution_state, UpdateExecutionState::Confirmed);
    assert!(
        store
            .transition(plan.operation_id, UpdateExecutionState::Completed)
            .is_err()
    );
    store
        .transition(plan.operation_id, UpdateExecutionState::ApplyingComponents)
        .unwrap();
    store
        .record_component(plan.operation_id, component(UpdateComponentState::Applied))
        .unwrap();
    let completed = store
        .transition(plan.operation_id, UpdateExecutionState::Completed)
        .unwrap();
    assert_eq!(completed.phase, UpdatePhase::Applied);
    assert!(completed.completed_at.is_some());

    let reopened = UpdateStore::new(&paths);
    assert_eq!(reopened.load(plan.operation_id).unwrap(), completed);
}

#[test]
fn cancelled_preview_can_be_removed_but_terminal_receipt_cannot() {
    let temp = tempfile::tempdir().unwrap();
    let paths = paths(&temp);
    let store = UpdateStore::new(&paths);
    let cancelled = future_plan();
    store.create(&cancelled).unwrap();

    store.remove_cancelled(cancelled.operation_id).unwrap();
    assert!(store.load(cancelled.operation_id).is_err());
    assert!(store.latest().unwrap().is_none());

    let terminal = future_plan();
    let token = terminal.confirmation_token().unwrap();
    store.create(&terminal).unwrap();
    store.confirm(terminal.operation_id, &token).unwrap();
    store
        .transition(
            terminal.operation_id,
            UpdateExecutionState::ApplyingComponents,
        )
        .unwrap();
    store
        .transition(terminal.operation_id, UpdateExecutionState::Failed)
        .unwrap();
    assert!(store.remove_cancelled(terminal.operation_id).is_err());
    assert!(store.load(terminal.operation_id).is_ok());
}

#[cfg(unix)]
#[test]
fn operation_path_never_follows_a_symbolic_link() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().unwrap();
    let paths = paths(&temp);
    fs::create_dir_all(&paths.state_dir).unwrap();
    let outside = temp.path().join("outside");
    fs::create_dir(&outside).unwrap();
    symlink(&outside, paths.state_dir.join("updates")).unwrap();

    let store = UpdateStore::new(&paths);
    assert!(store.create(&future_plan()).is_err());
    assert_eq!(fs::read_dir(outside).unwrap().count(), 0);
}
