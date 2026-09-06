use kb_app::{AdmissionAction, edit_admission, edit_config};

#[test]
fn setting_one_key_preserves_unrelated_text() {
    let before =
        "# owner note\nschema_version: \"v1.0\"\ncustom: keep # inline\nsearch:\n  mode: direct\n";
    let after = edit_config(before, "search.mode", Some("bm25")).unwrap();

    assert!(after.starts_with("# owner note\n"));
    assert!(after.contains("custom: keep # inline\n"));
    assert!(after.contains("search:\n  mode: bm25\n"));
}

#[test]
fn unsetting_one_key_preserves_sibling_fields() {
    let before = "schema_version: \"v1.0\"\nlimits:\n  max_file_bytes: 10\n  max_files_per_review: 20 # keep\n";
    let after = edit_config(before, "limits.max_file_bytes", None).unwrap();

    assert!(!after.contains("max_file_bytes"));
    assert!(after.contains("max_files_per_review: 20 # keep\n"));
}

#[test]
fn changing_admission_state_preserves_owner_comments() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir(temp.path().join("Notes")).unwrap();
    let before = "# owner note\nschema_version: \"v1.0\"\ndirectories:\n  - id: notes\n    path: Notes # keep\n    enabled: true\n";

    let after = edit_admission(
        before,
        temp.path(),
        &AdmissionAction::Disable {
            id: "notes".to_owned(),
        },
    )
    .unwrap();

    assert!(after.starts_with("# owner note\n"));
    assert!(after.contains("path: Notes # keep\n"));
    assert!(after.contains("enabled: false\n"));
}
