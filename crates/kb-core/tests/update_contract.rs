use std::{path::PathBuf, str::FromStr};

use kb_core::{
    OperationId, SchemaVersion, UpdateComponent, UpdateComponentKind, UpdateComponentState,
    UpdateConfirmationToken, UpdateConflict, UpdateExecutionState, UpdateFileAction,
    UpdateFileChange, UpdateOwnership, UpdatePhase, UpdatePlan, UpdatePlanState, UpdateScope,
    UpdateScopeMode,
};
use uuid::Uuid;

fn fixture_plan() -> UpdatePlan {
    let vault_id = Uuid::parse_str("7a72fc6d-77c0-4e0a-a326-53da584afd68").unwrap();
    UpdatePlan {
        kind: "update".into(),
        phase: UpdatePhase::Preview,
        state: UpdatePlanState::ReviewRequired,
        operation_id: OperationId::from_str("9aa69f15-94ca-49b8-89b1-15f76e6d8454").unwrap(),
        current_version: "0.1.3".into(),
        target_version: "0.1.4".into(),
        scope: UpdateScope {
            mode: UpdateScopeMode::CurrentVault,
            vaults: vec![vault_id],
        },
        excluded_vaults: Vec::new(),
        components: vec![UpdateComponent {
            id: format!("vault:{vault_id}:template"),
            kind: UpdateComponentKind::VaultTemplate,
            state: UpdateComponentState::Pending,
            vault_id: Some(vault_id),
            host: None,
            skill_scope: None,
            from_version: Some("v1.0".into()),
            to_version: Some("v1.1".into()),
            changes: vec![UpdateFileChange {
                path: PathBuf::from("KB.md"),
                ownership: UpdateOwnership::MarkedRegion,
                action: UpdateFileAction::Update,
                before_sha256: Some("1".repeat(64)),
                after_sha256: Some("2".repeat(64)),
                diff: Some("-old\n+new\n".into()),
            }],
            message: "Vault template v1.1 is available.".into(),
        }],
        conflicts: vec![UpdateConflict {
            component_id: "skill:codex:user".into(),
            path: Some(PathBuf::from(".codex/skills/kb-note/SKILL.md")),
            reason: "managed content was edited".into(),
            next_action: "Review the local edit and reinstall explicitly.".into(),
        }],
        skipped: vec!["external Skills remain externally managed".into()],
        untouched: vec!["ordinary notes and Wiki content".into()],
        created_at: "2026-09-15T00:00:00Z".into(),
        expires_at: "2026-09-16T00:00:00Z".into(),
    }
}

#[test]
fn update_plan_serializes_stable_identity_fields_and_token() {
    let plan = fixture_plan();
    let value = serde_json::to_value(&plan).unwrap();
    for field in [
        "kind",
        "phase",
        "operation_id",
        "current_version",
        "target_version",
        "scope",
        "components",
        "conflicts",
        "skipped",
        "untouched",
    ] {
        assert!(value.get(field).is_some(), "missing {field}");
    }

    let token = plan.confirmation_token().unwrap();
    assert_eq!(token.to_string().len(), 64);
    assert_eq!(
        UpdateConfirmationToken::from_str(&token.to_string()).unwrap(),
        token
    );
}

#[test]
fn confirmation_token_binds_scope_and_original_digests() {
    let original = fixture_plan();
    let original_token = original.confirmation_token().unwrap();

    let mut excluded = original.clone();
    excluded.excluded_vaults.push(original.scope.vaults[0]);
    assert_ne!(excluded.confirmation_token().unwrap(), original_token);

    let mut changed_digest = original;
    changed_digest.components[0].changes[0].before_sha256 = Some("3".repeat(64));
    assert_ne!(changed_digest.confirmation_token().unwrap(), original_token);
}

#[test]
fn validation_rejects_nondeterministic_component_order_and_bad_digests() {
    let mut plan = fixture_plan();
    let mut earlier = plan.components[0].clone();
    earlier.id = "a-component".into();
    plan.components.push(earlier);
    assert!(plan.validate().is_err());

    let mut plan = fixture_plan();
    plan.components[0].changes[0].after_sha256 = Some("not-a-digest".into());
    assert!(plan.validate().is_err());
}

#[test]
fn execution_states_use_the_public_wire_names() {
    assert_eq!(
        serde_json::to_string(&UpdateExecutionState::CompletedWithSkips).unwrap(),
        "\"completed_with_skips\""
    );
    assert_eq!(
        serde_json::to_string(&UpdateComponentState::RebuildRequired).unwrap(),
        "\"rebuild_required\""
    );
    assert_eq!(SchemaVersion::new(1, 0).to_string(), "v1.0");
}
