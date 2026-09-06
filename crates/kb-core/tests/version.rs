use kb_core::{SchemaCompatibility, SchemaVersion};

#[test]
fn schema_version_requires_v_major_minor() {
    assert_eq!(
        "v1.0".parse::<SchemaVersion>().unwrap(),
        SchemaVersion::new(1, 0)
    );
    assert!("1.0".parse::<SchemaVersion>().is_err());
    assert!("v1".parse::<SchemaVersion>().is_err());
    assert_eq!(
        serde_json::to_string(&SchemaVersion::new(1, 0)).unwrap(),
        "\"v1.0\""
    );
}

#[test]
fn compatibility_distinguishes_write_and_diagnostic_boundaries() {
    let current = SchemaVersion::new(1, 2);
    assert_eq!(
        SchemaVersion::new(1, 2).compatibility_with(current),
        SchemaCompatibility::Current
    );
    assert_eq!(
        SchemaVersion::new(1, 1).compatibility_with(current),
        SchemaCompatibility::OlderMigratable
    );
    assert_eq!(
        SchemaVersion::new(1, 3).compatibility_with(current),
        SchemaCompatibility::NewerMinorReadOnly
    );
    assert_eq!(
        SchemaVersion::new(2, 0).compatibility_with(current),
        SchemaCompatibility::NewerMajorDiagnosticOnly
    );
}
