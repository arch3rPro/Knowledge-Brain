use std::collections::BTreeMap;
use std::fs;

use kb_app::{
    AgentRoots, AppContext, InitRequest, UpdatePlanningOutcome, UpdateRuntime, UpdateSelection,
    UserPaths, confirm_update, init_and_register_vault, plan_update, resume_update,
};
use kb_core::{UpdateComponentKind, UpdateComponentState, UpdateExecutionState};

const V1_1_KB: &str = include_str!("../../../assets/vault-template-history/v1.1/KB.md");
const V1_1_MANIFEST: &str =
    include_str!("../../../assets/vault-template-history/v1.1/template.yml");

fn paths(temp: &tempfile::TempDir) -> UserPaths {
    UserPaths::new(
        temp.path().join("config"),
        temp.path().join("state"),
        temp.path().join("cache"),
    )
}

fn context(temp: &tempfile::TempDir, current: &std::path::Path) -> AppContext {
    AppContext::new(
        BTreeMap::from([
            (
                "KB_CONFIG_DIR".into(),
                temp.path().join("config").to_string_lossy().into_owned(),
            ),
            (
                "KB_STATE_DIR".into(),
                temp.path().join("state").to_string_lossy().into_owned(),
            ),
            (
                "KB_CACHE_DIR".into(),
                temp.path().join("cache").to_string_lossy().into_owned(),
            ),
            (
                "KB_AGENT_HOME".into(),
                temp.path().join("home").to_string_lossy().into_owned(),
            ),
            (
                "KB_AGENT_CONFIG_DIR".into(),
                temp.path()
                    .join("agent-config")
                    .to_string_lossy()
                    .into_owned(),
            ),
        ]),
        current.to_path_buf(),
    )
}

fn old_template(vault: &std::path::Path) {
    fs::write(vault.join("KB.md"), V1_1_KB).unwrap();
    fs::write(vault.join(".kb/template.yml"), V1_1_MANIFEST).unwrap();
}

fn prepare(
    temp: &tempfile::TempDir,
    paths: &UserPaths,
    vault: &std::path::Path,
) -> (kb_core::OperationId, kb_core::UpdateConfirmationToken) {
    let outcome = plan_update(
        &UpdateRuntime {
            identity: kb_update::BuildIdentity::development(env!("CARGO_PKG_VERSION")).unwrap(),
            executable: std::env::current_exe().unwrap(),
        },
        paths,
        &AgentRoots::new(temp.path().join("home"), temp.path().join("agent-config")),
        vault,
        &BTreeMap::new(),
        &UpdateSelection::default(),
        true,
    )
    .unwrap();
    match outcome {
        UpdatePlanningOutcome::Plan {
            plan,
            confirmation_token: Some(token),
        } => (plan.operation_id, token),
        _ => panic!("expected persisted plan"),
    }
}

#[test]
fn changed_template_becomes_stale_while_index_still_rebuilds() {
    let temp = tempfile::tempdir().unwrap();
    let paths = paths(&temp);
    let vault = init_and_register_vault(
        &InitRequest {
            target: temp.path().join("vault"),
        },
        &paths,
    )
    .unwrap();
    old_template(&vault.root);
    let (operation_id, token) = prepare(&temp, &paths, &vault.root);
    let changed = fs::read_to_string(vault.root.join("KB.md"))
        .unwrap()
        .replace(
            "<!-- kb:rules:start -->",
            "<!-- kb:rules:start -->\nCUSTOM MANAGED RULE",
        );
    fs::write(vault.root.join("KB.md"), &changed).unwrap();

    let result = confirm_update(&context(&temp, &vault.root), &token).unwrap();

    assert_eq!(result.operation_id, operation_id);
    assert_eq!(result.execution_state, UpdateExecutionState::Partial);
    assert_eq!(
        fs::read_to_string(vault.root.join("KB.md")).unwrap(),
        changed
    );
    assert!(vault.root.join(".kb/cache/catalog.json").is_file());
    assert_eq!(
        result
            .components
            .iter()
            .find(|component| component.kind == UpdateComponentKind::VaultTemplate)
            .unwrap()
            .state,
        UpdateComponentState::Stale
    );
}

#[test]
fn successful_components_are_not_repeated_on_resume() {
    let temp = tempfile::tempdir().unwrap();
    let paths = paths(&temp);
    let vault = init_and_register_vault(
        &InitRequest {
            target: temp.path().join("vault"),
        },
        &paths,
    )
    .unwrap();
    old_template(&vault.root);
    let (operation_id, token) = prepare(&temp, &paths, &vault.root);
    let context = context(&temp, &vault.root);
    let first = confirm_update(&context, &token).unwrap();
    let template_after = fs::read(vault.root.join(".kb/template.yml")).unwrap();
    let index_after = fs::read(vault.root.join(".kb/cache/catalog.json")).unwrap();

    let second = resume_update(&context, operation_id).unwrap();

    assert_eq!(first, second);
    assert_eq!(
        fs::read(vault.root.join(".kb/template.yml")).unwrap(),
        template_after
    );
    assert_eq!(
        fs::read(vault.root.join(".kb/cache/catalog.json")).unwrap(),
        index_after
    );
}

#[test]
fn index_failure_is_rebuild_required_without_undoing_template() {
    let temp = tempfile::tempdir().unwrap();
    let paths = paths(&temp);
    let vault = init_and_register_vault(
        &InitRequest {
            target: temp.path().join("vault"),
        },
        &paths,
    )
    .unwrap();
    old_template(&vault.root);
    let (_operation_id, token) = prepare(&temp, &paths, &vault.root);
    fs::remove_dir_all(vault.root.join(".kb/cache")).unwrap();
    fs::write(vault.root.join(".kb/cache"), b"blocks cache directory").unwrap();

    let result = confirm_update(&context(&temp, &vault.root), &token).unwrap();

    assert_eq!(result.execution_state, UpdateExecutionState::Partial);
    assert!(
        fs::read_to_string(vault.root.join(".kb/template.yml"))
            .unwrap()
            .contains("template_version: v1.2")
    );
    assert_eq!(
        result
            .components
            .iter()
            .find(|component| component.kind == UpdateComponentKind::SearchIndex)
            .unwrap()
            .state,
        UpdateComponentState::RebuildRequired
    );
}
