use std::fs;

use kb_app::{InitRequest, UserPaths, apply_vault_upgrade, create_vault_upgrade_plan, init_vault};
use kb_core::{ErrorCode, SchemaVersion};

const LEGACY_KB: &str = include_str!("../../../assets/vault-template-history/v1.0/KB.md");
const RELEASED_KB: &str =
    include_str!("../../../assets/vault-template-history/released-v0.1.x/KB.md");

fn setup() -> (tempfile::TempDir, std::path::PathBuf, UserPaths) {
    let temporary = tempfile::tempdir().unwrap();
    let vault = temporary.path().join("vault");
    init_vault(&InitRequest {
        target: vault.clone(),
    })
    .unwrap();
    let paths = UserPaths::new(
        temporary.path().join("config"),
        temporary.path().join("state"),
        temporary.path().join("cache"),
    );
    (temporary, vault, paths)
}

#[test]
fn legacy_default_vault_is_previewed_then_upgraded_without_touching_user_content() {
    let (_temporary, vault, paths) = setup();
    fs::remove_file(vault.join(".kb/template.yml")).unwrap();
    fs::write(vault.join("KB.md"), RELEASED_KB).unwrap();
    fs::create_dir(vault.join("Skills")).unwrap();
    fs::write(vault.join("Skills/user-note.md"), "# Mine\n").unwrap();

    let plan = create_vault_upgrade_plan(&vault, &paths).unwrap();

    assert_eq!(plan.from_template_version, None);
    assert_eq!(plan.to_template_version, SchemaVersion::new(1, 3));
    assert!(plan.conflicts.is_empty());
    assert!(plan.diff.contains("+++ KB.md"));
    assert!(plan.diff.contains("+<!-- kb:rules:start -->"));
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
    assert_eq!(plan.planned.len(), plan.writes.len());
    assert!(plan.changed.is_empty());
    assert!(plan.stale.is_empty());
    assert!(plan.skipped.is_empty());
    assert!(plan.failed.is_empty());
    assert_eq!(
        fs::read_to_string(vault.join("KB.md")).unwrap(),
        RELEASED_KB
    );
    assert!(!vault.join(".kb/template.yml").exists());

    let result = apply_vault_upgrade(&paths, plan.operation_id).unwrap();

    assert_eq!(result.template_version, SchemaVersion::new(1, 3));
    assert_eq!(result.from_template_version, None);
    assert_eq!(result.to_template_version, SchemaVersion::new(1, 3));
    assert_eq!(result.planned, result.changed);
    assert_eq!(result.unchanged.len(), 2);
    assert!(result.stale.is_empty());
    assert!(result.skipped.is_empty());
    assert!(result.failed.is_empty());
    assert!(
        fs::read_to_string(vault.join("KB.md"))
            .unwrap()
            .contains("<!-- kb:rules:start -->")
    );
    assert_eq!(
        fs::read_to_string(vault.join("Skills/user-note.md")).unwrap(),
        "# Mine\n"
    );
}

#[test]
fn customized_unmarked_rules_are_reported_without_creating_an_applicable_plan() {
    let (_temporary, vault, paths) = setup();
    fs::remove_file(vault.join(".kb/template.yml")).unwrap();
    fs::write(vault.join("KB.md"), "# My private rules\n").unwrap();

    let plan = create_vault_upgrade_plan(&vault, &paths).unwrap();

    assert_eq!(plan.conflicts.len(), 1);
    assert_eq!(plan.conflicts[0].path.as_str(), "KB.md");
    assert!(plan.writes.is_empty());
    let error = apply_vault_upgrade(&paths, plan.operation_id).unwrap_err();
    assert_eq!(error.code, ErrorCode::InvalidConfig);
    assert_eq!(
        fs::read_to_string(vault.join("KB.md")).unwrap(),
        "# My private rules\n"
    );
    assert!(!vault.join(".kb/template.yml").exists());
}

#[test]
fn upgrade_rejects_a_file_changed_after_preview() {
    let (_temporary, vault, paths) = setup();
    fs::remove_file(vault.join(".kb/template.yml")).unwrap();
    fs::write(vault.join("KB.md"), LEGACY_KB).unwrap();
    let plan = create_vault_upgrade_plan(&vault, &paths).unwrap();
    fs::write(vault.join("KB.md"), "changed after preview\n").unwrap();

    let error = apply_vault_upgrade(&paths, plan.operation_id).unwrap_err();

    assert_eq!(error.code, ErrorCode::PlanStale);
    assert_eq!(
        fs::read_to_string(vault.join("KB.md")).unwrap(),
        "changed after preview\n"
    );
    assert!(!vault.join(".kb/template.yml").exists());
}

#[test]
fn malformed_template_metadata_is_a_preserved_preview_conflict() {
    let (_temporary, vault, paths) = setup();
    fs::write(
        vault.join(".kb/template.yml"),
        "template_version: [broken\n",
    )
    .unwrap();

    let plan = create_vault_upgrade_plan(&vault, &paths).unwrap();

    assert!(plan.writes.is_empty());
    assert_eq!(plan.conflicts.len(), 1);
    assert_eq!(plan.conflicts[0].path.as_str(), ".kb/template.yml");
    assert_eq!(
        fs::read_to_string(vault.join(".kb/template.yml")).unwrap(),
        "template_version: [broken\n"
    );
}

#[test]
fn legacy_windows_line_endings_are_a_known_baseline() {
    let (_temporary, vault, paths) = setup();
    fs::remove_file(vault.join(".kb/template.yml")).unwrap();
    fs::write(vault.join("KB.md"), LEGACY_KB.replace('\n', "\r\n")).unwrap();

    let plan = create_vault_upgrade_plan(&vault, &paths).unwrap();

    assert!(plan.conflicts.is_empty());
    assert!(
        plan.writes
            .iter()
            .any(|write| write.path.as_str() == "KB.md")
    );
}

#[test]
fn modified_marked_rules_are_preserved_as_a_conflict() {
    let (_temporary, vault, paths) = setup();
    let changed = fs::read_to_string(vault.join("KB.md")).unwrap().replace(
        "- Use `kb capabilities` to discover available behavior.",
        "- stale product rule",
    ) + "\n# Personal rules\n\nKeep this paragraph.\n";
    fs::write(vault.join("KB.md"), changed).unwrap();

    let plan = create_vault_upgrade_plan(&vault, &paths).unwrap();
    assert_eq!(plan.conflicts.len(), 1);
    assert!(plan.writes.is_empty());

    let updated = fs::read_to_string(vault.join("KB.md")).unwrap();
    assert!(updated.contains("stale product rule"));
    assert!(updated.contains("# Personal rules\n\nKeep this paragraph."));
}

#[test]
fn modified_product_schema_blocks_the_whole_upgrade() {
    let (_temporary, vault, paths) = setup();
    let schema = vault.join(".kb/schemas/config.schema.json");
    fs::write(&schema, "{}\n").unwrap();

    let plan = create_vault_upgrade_plan(&vault, &paths).unwrap();

    assert!(plan.writes.is_empty());
    assert_eq!(plan.conflicts.len(), 1);
    assert_eq!(
        plan.conflicts[0].path.as_str(),
        ".kb/schemas/config.schema.json"
    );
    assert_eq!(fs::read_to_string(schema).unwrap(), "{}\n");
}

#[test]
fn newer_template_metadata_requires_a_newer_cli() {
    let (_temporary, vault, paths) = setup();
    let manifest = fs::read_to_string(vault.join(".kb/template.yml"))
        .unwrap()
        .replace("template_version: v1.3", "template_version: v2.0");
    fs::write(vault.join(".kb/template.yml"), manifest).unwrap();

    let error = create_vault_upgrade_plan(&vault, &paths).unwrap_err();

    assert_eq!(error.code, ErrorCode::SchemaTooNew);
}
