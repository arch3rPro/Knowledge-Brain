use std::fs;
use std::time::{Duration, Instant};

use assert_cmd::Command;
use kb_app::{StoredUpdateStage, UpdateStore, UserPaths};
use kb_core::{
    OperationId, UpdateComponent, UpdateComponentKind, UpdateComponentState, UpdateExecutionState,
    UpdateFileAction, UpdateFileChange, UpdateOwnership, UpdatePhase, UpdatePlan, UpdatePlanState,
    UpdateScope, UpdateScopeMode,
};
use sha2::{Digest, Sha256};

fn paths(temp: &tempfile::TempDir) -> UserPaths {
    UserPaths::new(
        temp.path().join("config"),
        temp.path().join("state"),
        temp.path().join("cache"),
    )
}

fn prepare_executable_update(
    temp: &tempfile::TempDir,
    staged_exists: bool,
) -> (UserPaths, UpdatePlan, std::path::PathBuf) {
    let paths = paths(temp);
    let store = UpdateStore::new(&paths);
    let source = assert_cmd::cargo::cargo_bin!("kb");
    let target = temp.path().join(if cfg!(windows) {
        "installed.exe"
    } else {
        "installed"
    });
    fs::write(&target, b"old executable").unwrap();
    let before = hex::encode(Sha256::digest(fs::read(&target).unwrap()));
    let after = hex::encode(Sha256::digest(fs::read(source).unwrap()));
    let plan = UpdatePlan {
        kind: "update".into(),
        phase: UpdatePhase::Preview,
        state: UpdatePlanState::ReviewRequired,
        operation_id: OperationId::new(),
        current_version: env!("CARGO_PKG_VERSION").into(),
        target_version: env!("CARGO_PKG_VERSION").into(),
        scope: UpdateScope {
            mode: UpdateScopeMode::RegisteredVaults,
            vaults: Vec::new(),
        },
        excluded_vaults: Vec::new(),
        components: vec![UpdateComponent {
            id: "executable".into(),
            kind: UpdateComponentKind::Executable,
            state: UpdateComponentState::Pending,
            vault_id: None,
            host: None,
            skill_scope: None,
            from_version: Some(env!("CARGO_PKG_VERSION").into()),
            to_version: Some(env!("CARGO_PKG_VERSION").into()),
            changes: vec![UpdateFileChange {
                path: target.clone(),
                ownership: UpdateOwnership::Executable,
                action: UpdateFileAction::Replace,
                before_sha256: Some(before),
                after_sha256: Some(after.clone()),
                diff: None,
            }],
            message: "Replace executable.".into(),
        }],
        conflicts: Vec::new(),
        skipped: Vec::new(),
        untouched: Vec::new(),
        created_at: "2026-01-01T00:00:00Z".into(),
        expires_at: "2099-01-01T00:00:00Z".into(),
    };
    let stage = store.create_stage().unwrap();
    fs::create_dir(stage.path().join("verified")).unwrap();
    if staged_exists {
        fs::copy(
            source,
            stage.path().join(if cfg!(windows) {
                "verified/kb.exe"
            } else {
                "verified/kb"
            }),
        )
        .unwrap();
    }
    store.create(&plan).unwrap();
    store
        .attach_stage(
            plan.operation_id,
            stage,
            &StoredUpdateStage {
                version: env!("CARGO_PKG_VERSION").into(),
                target: if cfg!(windows) {
                    "x86_64-pc-windows-msvc"
                } else if cfg!(target_arch = "aarch64") {
                    "aarch64-apple-darwin"
                } else {
                    "x86_64-unknown-linux-gnu"
                }
                .into(),
                executable_relative: std::path::PathBuf::from("stage/verified")
                    .join(if cfg!(windows) { "kb.exe" } else { "kb" }),
                executable_sha256: after,
                archive_sha256: "a".repeat(64),
            },
        )
        .unwrap();
    let token = plan.confirmation_token().unwrap();
    store.confirm(plan.operation_id, &token).unwrap();
    (paths, plan, target)
}

#[test]
fn replacement_helper_persists_and_resumes_to_terminal_state() {
    let temp = tempfile::tempdir().unwrap();
    let (paths, plan, target) = prepare_executable_update(&temp, true);

    let output = Command::cargo_bin("kb")
        .unwrap()
        .env("KB_CONFIG_DIR", &paths.config_dir)
        .env("KB_STATE_DIR", &paths.state_dir)
        .env("KB_CACHE_DIR", &paths.cache_dir)
        .args([
            "__replace-update",
            "--operation",
            &plan.operation_id.to_string(),
            "--parent-pid",
            &u32::MAX.to_string(),
            "--parent-start-time",
            "0",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());

    let store = UpdateStore::new(&paths);
    let deadline = Instant::now() + Duration::from_secs(5);
    let terminal = loop {
        let operation = store.load(plan.operation_id).unwrap();
        if operation.execution_state.is_terminal() {
            break operation;
        }
        assert!(
            Instant::now() < deadline,
            "resume did not reach a terminal state"
        );
        std::thread::sleep(Duration::from_millis(25));
    };
    assert_eq!(terminal.execution_state, UpdateExecutionState::Completed);
    assert_eq!(terminal.components[0].state, UpdateComponentState::Applied);
    assert_eq!(
        hex::encode(Sha256::digest(fs::read(target).unwrap())),
        terminal.components[0].changes[0]
            .after_sha256
            .as_deref()
            .unwrap()
    );
}

#[test]
fn failed_replacement_preserves_target_and_records_failed_state() {
    let temp = tempfile::tempdir().unwrap();
    let (paths, plan, target) = prepare_executable_update(&temp, false);
    let before = fs::read(&target).unwrap();

    let output = Command::cargo_bin("kb")
        .unwrap()
        .env("KB_CONFIG_DIR", &paths.config_dir)
        .env("KB_STATE_DIR", &paths.state_dir)
        .env("KB_CACHE_DIR", &paths.cache_dir)
        .args([
            "__replace-update",
            "--operation",
            &plan.operation_id.to_string(),
            "--parent-pid",
            &u32::MAX.to_string(),
            "--parent-start-time",
            "0",
        ])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert_eq!(fs::read(target).unwrap(), before);
    assert_eq!(
        UpdateStore::new(&paths)
            .load(plan.operation_id)
            .unwrap()
            .execution_state,
        UpdateExecutionState::Failed
    );
}

#[test]
fn replacement_rejects_an_executable_changed_after_confirmation() {
    let temp = tempfile::tempdir().unwrap();
    let (paths, plan, target) = prepare_executable_update(&temp, true);
    fs::write(&target, b"changed after confirmation").unwrap();

    let output = Command::cargo_bin("kb")
        .unwrap()
        .env("KB_CONFIG_DIR", &paths.config_dir)
        .env("KB_STATE_DIR", &paths.state_dir)
        .env("KB_CACHE_DIR", &paths.cache_dir)
        .args([
            "__replace-update",
            "--operation",
            &plan.operation_id.to_string(),
            "--parent-pid",
            &u32::MAX.to_string(),
            "--parent-start-time",
            "0",
        ])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert_eq!(fs::read(target).unwrap(), b"changed after confirmation");
    assert_eq!(
        UpdateStore::new(&paths)
            .load(plan.operation_id)
            .unwrap()
            .execution_state,
        UpdateExecutionState::Failed
    );
}
