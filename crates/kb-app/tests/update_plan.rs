use std::collections::BTreeMap;
use std::fs;

use kb_app::{
    AgentRoots, InitRequest, TargetPlanRequest, UpdatePlanningOutcome, UpdateRuntime,
    UpdateSelection, UserPaths, create_target_update_plan, init_and_register_vault,
    invoke_target_planner, plan_update, resolve_update_scope,
};
use kb_core::{UpdateComponentKind, UpdateComponentState, UpdatePlanState};

fn paths(temp: &tempfile::TempDir) -> UserPaths {
    UserPaths::new(
        temp.path().join("config"),
        temp.path().join("state"),
        temp.path().join("cache"),
    )
}

fn setup_two_vaults() -> (
    tempfile::TempDir,
    UserPaths,
    kb_app::InitReport,
    kb_app::InitReport,
) {
    let temp = tempfile::tempdir().unwrap();
    let paths = paths(&temp);
    let first = init_and_register_vault(
        &InitRequest {
            target: temp.path().join("first"),
        },
        &paths,
    )
    .unwrap();
    let second = init_and_register_vault(
        &InitRequest {
            target: temp.path().join("second"),
        },
        &paths,
    )
    .unwrap();
    (temp, paths, first, second)
}

#[test]
fn inside_vault_defaults_to_only_that_vault() {
    let (_temp, paths, first, _second) = setup_two_vaults();
    let current = first.root.join("Skills");

    let scope = resolve_update_scope(
        &paths,
        &current,
        &BTreeMap::new(),
        &UpdateSelection::default(),
    )
    .unwrap();

    assert_eq!(scope.vaults.len(), 1);
    assert_eq!(scope.vaults[0].vault_id, first.vault_id);
}

#[test]
fn outside_vault_uses_registered_vaults_and_supports_selection_and_exclusion() {
    let (temp, paths, first, second) = setup_two_vaults();
    let outside = temp.path();

    let all = resolve_update_scope(
        &paths,
        outside,
        &BTreeMap::new(),
        &UpdateSelection::default(),
    )
    .unwrap();
    assert_eq!(all.vaults.len(), 2);

    let selected = resolve_update_scope(
        &paths,
        outside,
        &BTreeMap::new(),
        &UpdateSelection {
            vault: Some(first.vault_id.to_string()),
            ..UpdateSelection::default()
        },
    )
    .unwrap();
    assert_eq!(selected.vaults[0].vault_id, first.vault_id);

    let excluded = resolve_update_scope(
        &paths,
        outside,
        &BTreeMap::new(),
        &UpdateSelection {
            excluded_vaults: vec![second.vault_id],
            ..UpdateSelection::default()
        },
    )
    .unwrap();
    assert_eq!(excluded.vaults.len(), 1);
    assert_eq!(excluded.vaults[0].vault_id, first.vault_id);
    assert_eq!(excluded.excluded_vaults, vec![second.vault_id]);
}

#[test]
fn stale_registry_paths_are_reported_and_external_or_missing_skills_are_not_adopted() {
    let (temp, paths, first, second) = setup_two_vaults();
    let moved = temp.path().join("moved-second");
    fs::rename(&second.root, &moved).unwrap();
    let external = first.root.join(".agents/skills/kb-vault");
    fs::create_dir_all(&external).unwrap();
    fs::write(external.join("SKILL.md"), "external\n").unwrap();

    let scope = resolve_update_scope(
        &paths,
        temp.path(),
        &BTreeMap::new(),
        &UpdateSelection::default(),
    )
    .unwrap();

    assert_eq!(scope.vaults.len(), 1);
    assert_eq!(scope.vaults[0].vault_id, first.vault_id);
    assert_eq!(scope.managed_skills.len(), 0);
    assert_eq!(scope.skipped_vaults.len(), 1);
    assert_eq!(scope.skipped_vaults[0].vault_id, second.vault_id);
}

#[test]
fn target_plan_lists_template_index_executable_and_untouched_boundaries() {
    let (temp, paths, first, _second) = setup_two_vaults();
    let scope = resolve_update_scope(
        &paths,
        &first.root,
        &BTreeMap::new(),
        &UpdateSelection::default(),
    )
    .unwrap();
    let mut request = TargetPlanRequest::new(
        env!("CARGO_PKG_VERSION"),
        env!("CARGO_PKG_VERSION"),
        temp.path().join("kb"),
        scope,
        paths,
        AgentRoots::new(temp.path().join("home"), temp.path().join("agent-config")),
    )
    .unwrap();
    request.executable_before_sha256 = Some("a".repeat(64));
    request.executable_after_sha256 = Some("a".repeat(64));

    let plan = create_target_update_plan(&request).unwrap();

    assert_eq!(plan.state, UpdatePlanState::ReviewRequired);
    assert_eq!(plan.components.len(), 3);
    assert_eq!(plan.components[0].kind, UpdateComponentKind::Executable);
    assert_eq!(
        plan.components
            .iter()
            .find(|component| component.kind == UpdateComponentKind::SearchIndex)
            .unwrap()
            .state,
        UpdateComponentState::RebuildRequired
    );
    assert!(plan.untouched.contains(&"Wiki content".to_owned()));
    assert!(plan.untouched.contains(&"Obsidian settings".to_owned()));
    assert!(plan.untouched.contains(&"external Skills".to_owned()));
    assert!(!temp.path().join("state/updates").exists());
}

#[test]
fn target_plan_request_rejects_unrelated_environment_values() {
    let (temp, paths, first, _second) = setup_two_vaults();
    let scope = resolve_update_scope(
        &paths,
        &first.root,
        &BTreeMap::new(),
        &UpdateSelection::default(),
    )
    .unwrap();
    let mut request = TargetPlanRequest::new(
        env!("CARGO_PKG_VERSION"),
        env!("CARGO_PKG_VERSION"),
        temp.path().join("kb"),
        scope,
        paths,
        AgentRoots::new(temp.path().join("home"), temp.path().join("agent-config")),
    )
    .unwrap();
    request
        .environment
        .insert("SERVICE_TOKEN".into(), "must-not-persist".into());

    assert!(create_target_update_plan(&request).is_err());
}

#[cfg(unix)]
#[test]
fn verified_target_plan_is_used_without_substituting_running_assets() {
    use std::os::unix::fs::PermissionsExt;

    let (temp, paths, first, _second) = setup_two_vaults();
    let scope = resolve_update_scope(
        &paths,
        &first.root,
        &BTreeMap::new(),
        &UpdateSelection::default(),
    )
    .unwrap();
    let request = TargetPlanRequest::new(
        env!("CARGO_PKG_VERSION"),
        env!("CARGO_PKG_VERSION"),
        temp.path().join("kb"),
        scope,
        paths,
        AgentRoots::new(temp.path().join("home"), temp.path().join("agent-config")),
    )
    .unwrap();
    let mut target_plan = create_target_update_plan(&request).unwrap();
    target_plan.components[0].message = "sentinel from verified target".into();

    let fixture = temp.path().join("fixture-plan.json");
    fs::write(&fixture, serde_json::to_vec_pretty(&target_plan).unwrap()).unwrap();
    let executable = temp.path().join("fake-target");
    fs::write(
        &executable,
        "#!/bin/sh\nbase=$(dirname \"$0\")\nprintf invoked > \"$base/invoked\"\ncp \"$base/fixture-plan.json\" \"$5\"\n",
    )
    .unwrap();
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();

    let returned = invoke_target_planner(&executable, &request).unwrap();

    assert_eq!(returned, target_plan);
    assert!(temp.path().join("invoked").is_file());
    assert_eq!(
        returned.components[0].message,
        "sentinel from verified target"
    );
}

#[test]
fn prepare_persists_once_and_returns_existing_nonterminal_operation() {
    let (temp, paths, first, _second) = setup_two_vaults();
    let runtime = UpdateRuntime {
        identity: kb_update::BuildIdentity::development(env!("CARGO_PKG_VERSION")).unwrap(),
        executable: std::env::current_exe().unwrap(),
    };
    let roots = AgentRoots::new(temp.path().join("home"), temp.path().join("agent-config"));

    let first_outcome = plan_update(
        &runtime,
        &paths,
        &roots,
        &first.root,
        &BTreeMap::new(),
        &UpdateSelection::default(),
        true,
    )
    .unwrap();
    let operation_id = match first_outcome {
        UpdatePlanningOutcome::Plan {
            plan,
            confirmation_token,
        } => {
            assert!(confirmation_token.is_some());
            plan.operation_id
        }
        UpdatePlanningOutcome::Existing { .. } => panic!("first prepare returned existing"),
    };

    let second_outcome = plan_update(
        &runtime,
        &paths,
        &roots,
        &first.root,
        &BTreeMap::new(),
        &UpdateSelection::default(),
        true,
    )
    .unwrap();
    match second_outcome {
        UpdatePlanningOutcome::Existing { operation } => {
            assert_eq!(operation.operation_id, operation_id);
        }
        UpdatePlanningOutcome::Plan { .. } => panic!("second prepare created another plan"),
    }
}

#[test]
fn check_does_not_persist_a_local_plan() {
    let (temp, paths, first, _second) = setup_two_vaults();
    let runtime = UpdateRuntime {
        identity: kb_update::BuildIdentity::development(env!("CARGO_PKG_VERSION")).unwrap(),
        executable: std::env::current_exe().unwrap(),
    };

    let outcome = plan_update(
        &runtime,
        &paths,
        &AgentRoots::new(temp.path().join("home"), temp.path().join("agent-config")),
        &first.root,
        &BTreeMap::new(),
        &UpdateSelection::default(),
        false,
    )
    .unwrap();

    assert!(matches!(outcome, UpdatePlanningOutcome::Plan { .. }));
    assert!(!paths.state_dir.join("updates").exists());
}
