use std::{fs, path::Path};

use kb_app::{
    InitRequest, TemplateCompatibility, apply_template_update, init_vault, inspect_template,
    plan_template_update,
};
use kb_core::{SchemaVersion, UpdateComponentState, UpdateFileAction};

const LEGACY_KB: &str = include_str!("../../../assets/vault-template-history/v1.0/KB.md");
const V1_1_KB: &str = include_str!("../../../assets/vault-template-history/v1.1/KB.md");
const V1_1_MANIFEST: &str =
    include_str!("../../../assets/vault-template-history/v1.1/template.yml");
const V1_2_KB: &str = include_str!("../../../assets/vault-template-history/v1.2/KB.md");
const V1_2_MANIFEST: &str =
    include_str!("../../../assets/vault-template-history/v1.2/template.yml");

fn setup() -> (tempfile::TempDir, std::path::PathBuf) {
    let temporary = tempfile::tempdir().unwrap();
    let vault = temporary.path().join("vault");
    init_vault(&InitRequest {
        target: vault.clone(),
    })
    .unwrap();
    (temporary, vault)
}

#[test]
fn current_manifest_reports_current_template() {
    let (_temporary, vault) = setup();

    let inspection = inspect_template(&vault).unwrap();

    assert_eq!(inspection.version, Some(SchemaVersion::new(1, 3)));
    assert_eq!(inspection.latest, SchemaVersion::new(1, 3));
    assert_eq!(inspection.compatibility, TemplateCompatibility::Current);
    assert!(!inspection.upgrade_available);
    assert!(inspection.modified_entries.is_empty());
}

#[test]
fn complete_previous_manifest_is_a_safe_outdated_baseline() {
    let (_temporary, vault) = setup();
    fs::write(vault.join("KB.md"), V1_1_KB).unwrap();
    fs::write(vault.join(".kb/template.yml"), V1_1_MANIFEST).unwrap();

    let inspection = inspect_template(&vault).unwrap();
    let plan = plan_template_update(&vault).unwrap();

    assert_eq!(inspection.version, Some(SchemaVersion::new(1, 1)));
    assert_eq!(inspection.compatibility, TemplateCompatibility::Outdated);
    assert!(inspection.upgrade_available);
    assert!(plan.conflicts.is_empty());
    assert!(
        plan.writes
            .iter()
            .any(|write| write.path.as_str() == "KB.md")
    );
    assert!(
        plan.writes
            .iter()
            .any(|write| write.path.as_str() == ".kb/template.yml")
    );
}

#[test]
fn complete_v1_2_manifest_is_a_safe_outdated_baseline() {
    let (_temporary, vault) = setup();
    fs::write(vault.join("KB.md"), V1_2_KB).unwrap();
    fs::write(vault.join(".kb/template.yml"), V1_2_MANIFEST).unwrap();

    let plan = plan_template_update(&vault).unwrap();

    assert_eq!(plan.from_template_version, Some(SchemaVersion::new(1, 2)));
    assert_eq!(plan.to_template_version, SchemaVersion::new(1, 3));
    assert_eq!(plan.compatibility, TemplateCompatibility::Outdated);
    assert!(plan.conflicts.is_empty());
    assert!(
        plan.writes
            .iter()
            .any(|write| write.path.as_str() == "KB.md")
    );
    assert!(
        plan.writes
            .iter()
            .any(|write| write.path.as_str() == ".kb/template.yml")
    );
}

#[test]
fn semantically_current_manifest_allows_yaml_formatting_changes() {
    let (_temporary, vault) = setup();
    let manifest = fs::read_to_string(vault.join(".kb/template.yml")).unwrap();
    fs::write(
        vault.join(".kb/template.yml"),
        format!("# local YAML formatting is irrelevant\n{manifest}"),
    )
    .unwrap();

    let plan = plan_template_update(&vault).unwrap();

    assert!(plan.conflicts.is_empty());
    assert!(plan.writes.is_empty());
    assert_eq!(plan.component_state, UpdateComponentState::Unchanged);
}

#[test]
fn unregistered_manifest_digest_is_not_trusted_as_an_ownership_baseline() {
    let (_temporary, vault) = setup();
    let manifest = fs::read_to_string(vault.join(".kb/template.yml"))
        .unwrap()
        .replacen(
            "sha256: ",
            &format!("sha256: {} # replaced\n# original: ", "a".repeat(64)),
            1,
        );
    fs::write(vault.join(".kb/template.yml"), manifest).unwrap();

    let plan = plan_template_update(&vault).unwrap();

    assert_eq!(plan.component_state, UpdateComponentState::Skipped);
    assert!(plan.writes.is_empty());
    assert!(plan.conflicts[0].reason.contains("recognized"));
}

#[test]
fn modified_marked_rules_skip_the_complete_template() {
    let (_temporary, vault) = setup();
    let changed = fs::read_to_string(vault.join("KB.md"))
        .unwrap()
        .replace("- Use `kb capabilities`", "- Never use `kb capabilities`");
    fs::write(vault.join("KB.md"), changed).unwrap();

    let plan = plan_template_update(&vault).unwrap();

    assert_eq!(plan.component_state, UpdateComponentState::Skipped);
    assert!(plan.writes.is_empty());
    assert_eq!(plan.conflicts.len(), 1);
    assert_eq!(plan.conflicts[0].path.as_ref().unwrap(), Path::new("KB.md"));
}

#[test]
fn user_content_outside_an_unchanged_managed_region_is_preserved() {
    let (_temporary, vault) = setup();
    let before = fs::read_to_string(vault.join("KB.md")).unwrap()
        + "\n# Personal rules\n\nKeep this paragraph.\n";
    fs::write(vault.join("KB.md"), &before).unwrap();

    let plan = plan_template_update(&vault).unwrap();

    assert!(plan.conflicts.is_empty());
    assert_eq!(plan.component_state, UpdateComponentState::Unchanged);
    assert!(plan.writes.is_empty());
    assert_eq!(fs::read_to_string(vault.join("KB.md")).unwrap(), before);
}

#[test]
fn malformed_markers_report_literal_repair_boundaries() {
    let (_temporary, vault) = setup();
    fs::write(
        vault.join("KB.md"),
        "<!-- kb:rules:start -->\none\n<!-- kb:rules:start -->\ntwo\n<!-- kb:rules:end -->\n",
    )
    .unwrap();

    let plan = plan_template_update(&vault).unwrap();

    assert_eq!(plan.component_state, UpdateComponentState::Skipped);
    assert!(plan.writes.is_empty());
    assert!(
        plan.conflicts[0]
            .next_action
            .contains("<!-- kb:rules:start -->")
    );
    assert!(
        plan.conflicts[0]
            .next_action
            .contains("<!-- kb:rules:end -->")
    );
    assert!(plan.conflicts[0].next_action.contains("outside"));
}

#[test]
fn known_unmanaged_historical_rules_can_gain_a_manifest() {
    let (_temporary, vault) = setup();
    fs::remove_file(vault.join(".kb/template.yml")).unwrap();
    fs::write(vault.join("KB.md"), LEGACY_KB).unwrap();

    let plan = plan_template_update(&vault).unwrap();

    assert_eq!(plan.component_state, UpdateComponentState::Pending);
    assert!(plan.conflicts.is_empty());
    assert!(
        plan.writes
            .iter()
            .any(|write| write.path.as_str() == "KB.md")
    );
    assert!(
        plan.writes
            .iter()
            .any(|write| write.path.as_str() == ".kb/template.yml")
    );

    let result = apply_template_update(&vault, &plan).unwrap();
    assert_eq!(result.to_template_version, SchemaVersion::new(1, 3));
    assert!(!result.changed.is_empty());
    assert!(result.stale.is_empty());
    assert!(result.skipped.is_empty());
    assert!(result.failed.is_empty());
    assert!(result.changed.iter().any(|path| path.as_str() == "KB.md"));
}

#[test]
fn modified_owned_schema_skips_all_template_writes() {
    let (_temporary, vault) = setup();
    fs::write(vault.join(".kb/schemas/config.schema.json"), "{}\n").unwrap();

    let plan = plan_template_update(&vault).unwrap();

    assert_eq!(plan.component_state, UpdateComponentState::Skipped);
    assert!(plan.writes.is_empty());
    assert_eq!(
        plan.conflicts[0].path.as_ref().unwrap(),
        Path::new(".kb/schemas/config.schema.json")
    );
}

#[test]
fn missing_manifest_owned_schema_is_recreated() {
    let (_temporary, vault) = setup();
    fs::remove_file(vault.join(".kb/schemas/config.schema.json")).unwrap();

    let plan = plan_template_update(&vault).unwrap();

    assert_eq!(plan.component_state, UpdateComponentState::Pending);
    assert!(plan.conflicts.is_empty());
    assert_eq!(plan.changes[0].action, UpdateFileAction::Create);
}
