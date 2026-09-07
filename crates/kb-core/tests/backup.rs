use kb_core::{BackupFile, BackupManifest, ErrorCode, PortableRelativePath, SchemaVersion};
use uuid::Uuid;

fn path(value: &str) -> PortableRelativePath {
    PortableRelativePath::parse(value).unwrap()
}

fn manifest() -> BackupManifest {
    BackupManifest {
        schema_version: SchemaVersion::new(1, 0),
        vault_schema_version: SchemaVersion::new(1, 0),
        vault_id: Uuid::nil(),
        created_at: "2026-09-07T00:00:00Z".into(),
        app_version: "0.1.0".into(),
        complete_source_evidence: true,
        directories: vec![path("Wiki"), path("Wiki/articles")],
        files: vec![BackupFile {
            path: path("Wiki/articles/a.md"),
            size: 3,
            sha256: "a".repeat(64),
        }],
    }
}

#[test]
fn backup_manifest_requires_sorted_unique_portable_entries_and_hashes() {
    manifest().validate().unwrap();

    let mut unsorted = manifest();
    unsorted.directories.reverse();
    assert_eq!(
        unsorted.validate().unwrap_err().code,
        ErrorCode::InvalidConfig
    );

    let mut duplicate = manifest();
    duplicate.files.push(duplicate.files[0].clone());
    assert_eq!(
        duplicate.validate().unwrap_err().code,
        ErrorCode::InvalidConfig
    );

    let mut invalid_hash = manifest();
    invalid_hash.files[0].sha256 = "A".repeat(64);
    assert_eq!(
        invalid_hash.validate().unwrap_err().code,
        ErrorCode::InvalidConfig
    );

    let mut type_conflict = manifest();
    type_conflict.directories.push(path("Wiki/articles/a.md"));
    type_conflict.directories.sort();
    assert_eq!(
        type_conflict.validate().unwrap_err().code,
        ErrorCode::InvalidConfig
    );
}
