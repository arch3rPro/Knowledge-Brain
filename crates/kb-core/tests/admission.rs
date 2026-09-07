use kb_core::{AdmissionDocument, ErrorCode};

#[test]
fn admission_validates_disabled_entries_and_supplies_effective_filters() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir(temp.path().join("Reading")).unwrap();
    let valid: AdmissionDocument = serde_yaml_ng::from_str(
        "schema_version: \"v1.0\"\ndirectories:\n  - id: reading\n    path: Reading\n    enabled: false\n",
    )
    .unwrap();
    valid.validate(temp.path()).unwrap();
    let includes = valid.directories[0].effective_include();
    for pattern in [
        "**/*.md",
        "**/*.txt",
        "**/*.html",
        "**/*.htm",
        "**/*.epub",
        "**/*.docx",
        "**/*.pdf",
    ] {
        assert!(
            includes.contains(&pattern),
            "missing default pattern {pattern}"
        );
    }
    assert!(
        valid.directories[0]
            .effective_exclude()
            .contains(&"**/.git/**")
    );

    let invalid: AdmissionDocument = serde_yaml_ng::from_str(
        "schema_version: \"v1.0\"\ndirectories:\n  - id: nested\n    path: Reading/private\n    enabled: false\n",
    )
    .unwrap();
    let error = invalid.validate(temp.path()).unwrap_err();
    assert_eq!(error.code, ErrorCode::UnsafePath);
}

#[test]
fn admission_rejects_duplicate_ids_and_portable_paths() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir(temp.path().join("Reading")).unwrap();
    let duplicate_ids: AdmissionDocument = serde_yaml_ng::from_str(
        "schema_version: \"v1.0\"\ndirectories:\n  - { id: same, path: Reading, enabled: true }\n  - { id: same, path: READING, enabled: true }\n",
    )
    .unwrap();
    assert!(duplicate_ids.validate(temp.path()).is_err());

    let duplicate_paths: AdmissionDocument = serde_yaml_ng::from_str(
        "schema_version: \"v1.0\"\ndirectories:\n  - { id: one, path: Reading, enabled: true }\n  - { id: two, path: READING, enabled: true }\n",
    )
    .unwrap();
    assert!(duplicate_paths.validate(temp.path()).is_err());
}
