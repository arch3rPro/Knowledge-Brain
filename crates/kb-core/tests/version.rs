use kb_core::{
    MigrationCatalog, MigrationStep, SchemaCompatibility, SchemaRelation, SchemaVersion,
};

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
fn schema_relation_distinguishes_numeric_version_boundaries() {
    let current = SchemaVersion::new(1, 2);
    assert_eq!(
        SchemaVersion::new(1, 2).relation_to(current),
        SchemaRelation::Current
    );
    assert_eq!(
        SchemaVersion::new(1, 1).relation_to(current),
        SchemaRelation::Older
    );
    assert_eq!(
        SchemaVersion::new(1, 3).relation_to(current),
        SchemaRelation::NewerMinor
    );
    assert_eq!(
        SchemaVersion::new(2, 0).relation_to(current),
        SchemaRelation::NewerMajor
    );
}

#[test]
fn migration_catalog_requires_a_complete_forward_path() {
    let current = SchemaVersion::new(1, 2);
    let catalog = MigrationCatalog::new([
        MigrationStep::new(SchemaVersion::new(1, 0), SchemaVersion::new(1, 1)),
        MigrationStep::new(SchemaVersion::new(1, 1), current),
    ])
    .unwrap();

    assert_eq!(
        catalog.classify(SchemaVersion::new(1, 0), current),
        SchemaCompatibility::OlderMigratable,
    );
    assert_eq!(
        MigrationCatalog::empty().classify(SchemaVersion::new(1, 0), current),
        SchemaCompatibility::OlderUnsupported,
    );
}

#[test]
fn migration_catalog_accepts_a_direct_route_to_current() {
    let current = SchemaVersion::new(1, 2);
    let catalog =
        MigrationCatalog::new([MigrationStep::new(SchemaVersion::new(1, 0), current)]).unwrap();

    assert_eq!(
        catalog.classify(SchemaVersion::new(1, 0), current),
        SchemaCompatibility::OlderMigratable,
    );
}

#[test]
fn migration_catalog_rejects_a_route_missing_its_final_intermediate() {
    let current = SchemaVersion::new(1, 2);
    let catalog = MigrationCatalog::new([MigrationStep::new(
        SchemaVersion::new(1, 0),
        SchemaVersion::new(1, 1),
    )])
    .unwrap();

    assert_eq!(
        catalog.classify(SchemaVersion::new(1, 0), current),
        SchemaCompatibility::OlderUnsupported,
    );
}

#[test]
fn migration_catalog_rejects_non_deterministic_or_non_forward_steps() {
    let v1_0 = SchemaVersion::new(1, 0);
    let v1_1 = SchemaVersion::new(1, 1);
    assert!(MigrationCatalog::new([MigrationStep::new(v1_0, v1_0)]).is_err());
    assert!(MigrationCatalog::new([MigrationStep::new(v1_1, v1_0)]).is_err());
    assert!(
        MigrationCatalog::new([
            MigrationStep::new(v1_0, v1_1),
            MigrationStep::new(v1_0, SchemaVersion::new(1, 2)),
        ])
        .is_err()
    );
}
