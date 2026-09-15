use std::fs;

use kb_app::{
    ConfigOverrides, InitRequest, TemplateCompatibility, UserPaths, init_vault, vault_status,
};
use kb_core::{SchemaCompatibility, SchemaVersion};

const V1_1_KB: &str = include_str!("../../../assets/vault-template-history/v1.1/KB.md");
const V1_1_MANIFEST: &str =
    include_str!("../../../assets/vault-template-history/v1.1/template.yml");

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
fn current_data_schema_and_old_template_are_reported_independently() {
    let (_temporary, vault, paths) = setup();
    fs::write(vault.join("KB.md"), V1_1_KB).unwrap();
    fs::write(vault.join(".kb/template.yml"), V1_1_MANIFEST).unwrap();

    let report = vault_status(&vault, &paths, &ConfigOverrides::default()).unwrap();

    assert_eq!(report.schema.compatibility, SchemaCompatibility::Current);
    assert_eq!(report.template.version, Some(SchemaVersion::new(1, 1)));
    assert_eq!(report.template.latest, SchemaVersion::new(1, 3));
    assert_eq!(
        report.template.compatibility,
        TemplateCompatibility::Outdated
    );
    assert!(report.template.upgrade_available);
    assert!(report.template.modified_entries.is_empty());
}

#[test]
fn modified_and_malformed_template_entries_are_visible() {
    let (_temporary, vault, paths) = setup();
    let changed = fs::read_to_string(vault.join("KB.md"))
        .unwrap()
        .replace("- Use `kb capabilities`", "- Never use `kb capabilities`");
    fs::write(vault.join("KB.md"), changed).unwrap();

    let modified = vault_status(&vault, &paths, &ConfigOverrides::default()).unwrap();
    assert_eq!(
        modified.template.compatibility,
        TemplateCompatibility::Modified
    );
    assert_eq!(modified.template.modified_entries, vec!["KB.md"]);

    fs::write(
        vault.join(".kb/template.yml"),
        "template_version: [broken\n",
    )
    .unwrap();
    let malformed = vault_status(&vault, &paths, &ConfigOverrides::default()).unwrap();
    assert_eq!(
        malformed.template.compatibility,
        TemplateCompatibility::Malformed
    );
    assert_eq!(
        malformed.template.modified_entries,
        vec![".kb/template.yml"]
    );
}

#[test]
fn structurally_valid_but_unrecognized_manifest_is_unknown() {
    let (_temporary, vault, paths) = setup();
    let manifest = fs::read_to_string(vault.join(".kb/template.yml"))
        .unwrap()
        .replacen(
            "sha256: ",
            &format!("sha256: {} # replaced\n# original: ", "a".repeat(64)),
            1,
        );
    fs::write(vault.join(".kb/template.yml"), manifest).unwrap();

    let report = vault_status(&vault, &paths, &ConfigOverrides::default()).unwrap();

    assert_eq!(
        report.template.compatibility,
        TemplateCompatibility::Unknown
    );
}
