use std::{fs, io::Write, path::Path};

use kb_app::{BackupCreateRequest, create_backup, restore_backup, verify_backup};
use kb_core::{BackupManifest, ErrorCode};
use time::OffsetDateTime;
use zip::{CompressionMethod, ZipArchive, ZipWriter, write::SimpleFileOptions};

fn setup() -> (tempfile::TempDir, std::path::PathBuf) {
    let temporary = tempfile::tempdir().unwrap();
    let vault = temporary.path().join("vault");
    kb_app::init_vault(&kb_app::InitRequest {
        target: vault.clone(),
    })
    .unwrap();
    fs::create_dir(vault.join("Notes")).unwrap();
    fs::create_dir(vault.join("Disabled")).unwrap();
    fs::write(vault.join("Notes/hello.md"), "portable bytes").unwrap();
    fs::create_dir(vault.join("Notes/.git")).unwrap();
    fs::write(vault.join("Notes/.git/secret"), "excluded").unwrap();
    fs::write(
        vault.join("admission.yml"),
        "schema_version: v1.0\ndirectories:\n  - id: notes\n    path: Notes\n    enabled: true\n  - id: disabled\n    path: Disabled\n    enabled: false\n",
    )
    .unwrap();
    fs::create_dir_all(vault.join("Wiki/external-sources/.objects/sha256/aa")).unwrap();
    fs::write(
        vault.join("Wiki/external-sources/.objects/sha256/aa/evidence"),
        b"evidence",
    )
    .unwrap();
    (temporary, vault)
}

fn create(vault: &Path, archive: &Path, include_source_objects: bool) {
    create_backup(&BackupCreateRequest {
        vault: vault.to_path_buf(),
        output: archive.to_path_buf(),
        include_source_objects,
        created_at: OffsetDateTime::from_unix_timestamp(1_788_739_200).unwrap(),
    })
    .unwrap();
}

#[test]
fn create_collects_exact_portable_scope_and_marks_compact_backups() {
    let (temporary, vault) = setup();
    let complete = temporary.path().join("complete.zip");
    create(&vault, &complete, true);
    let report = verify_backup(&complete).unwrap();
    assert!(report.complete_source_evidence);

    let complete_manifest = manifest(&complete);
    assert!(
        complete_manifest
            .directories
            .iter()
            .any(|path| path.as_str() == "Disabled")
    );
    assert!(
        complete_manifest
            .files
            .iter()
            .any(|file| file.path.as_str() == "Notes/hello.md")
    );
    assert!(
        complete_manifest
            .files
            .iter()
            .any(|file| file.path.as_str().contains("evidence"))
    );
    assert!(
        complete_manifest
            .files
            .iter()
            .all(|file| !file.path.as_str().contains("/.git/"))
    );
    assert!(
        complete_manifest
            .files
            .iter()
            .all(|file| !file.path.as_str().starts_with(".kb/cache/"))
    );

    let compact = temporary.path().join("compact.zip");
    create(&vault, &compact, false);
    let compact_manifest = manifest(&compact);
    assert!(!compact_manifest.complete_source_evidence);
    assert!(compact_manifest.files.iter().all(|file| {
        !file
            .path
            .as_str()
            .starts_with("Wiki/external-sources/.objects/")
    }));

    assert_eq!(
        create_backup(&BackupCreateRequest {
            vault: vault.clone(),
            output: compact,
            include_source_objects: true,
            created_at: OffsetDateTime::UNIX_EPOCH,
        })
        .unwrap_err()
        .code,
        ErrorCode::TargetNotEmpty
    );
    assert_eq!(
        create_backup(&BackupCreateRequest {
            vault: vault.clone(),
            output: vault.join("inside.zip"),
            include_source_objects: true,
            created_at: OffsetDateTime::UNIX_EPOCH,
        })
        .unwrap_err()
        .code,
        ErrorCode::UnsafePath
    );
    assert!(!vault.join("inside.zip").exists());
}

#[cfg(unix)]
#[test]
fn create_rejects_links_in_included_trees() {
    use std::os::unix::fs::symlink;

    let (temporary, vault) = setup();
    symlink(vault.join("KB.md"), vault.join("Notes/linked.md")).unwrap();
    let output = temporary.path().join("linked.zip");
    assert_eq!(
        create_backup(&BackupCreateRequest {
            vault,
            output: output.clone(),
            include_source_objects: true,
            created_at: OffsetDateTime::UNIX_EPOCH,
        })
        .unwrap_err()
        .code,
        ErrorCode::UnsafePath
    );
    assert!(!output.exists());
}

#[test]
fn verify_rejects_extra_traversal_and_hash_mismatch() {
    let (temporary, vault) = setup();
    let valid = temporary.path().join("valid.zip");
    create(&vault, &valid, true);
    let mut wrong_manifest = manifest(&valid);
    wrong_manifest.files[0].sha256 = "0".repeat(64);
    let wrong_hash = temporary.path().join("wrong-hash.zip");
    rewrite_archive(&valid, &wrong_hash, &wrong_manifest);
    assert_eq!(
        verify_backup(&wrong_hash).unwrap_err().code,
        ErrorCode::RestoreFailed
    );

    let archive_manifest = manifest(&valid);
    let malicious = temporary.path().join("malicious.zip");
    let file = fs::File::create(&malicious).unwrap();
    let mut writer = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    writer.start_file("manifest.json", options).unwrap();
    writer
        .write_all(&serde_json::to_vec(&archive_manifest).unwrap())
        .unwrap();
    writer.start_file("../escape", options).unwrap();
    writer.write_all(b"bad").unwrap();
    writer.finish().unwrap();

    assert_eq!(
        verify_backup(&malicious).unwrap_err().code,
        ErrorCode::RestoreFailed
    );
    assert!(!temporary.path().join("escape").exists());

    let linked = temporary.path().join("linked.zip");
    let file = fs::File::create(&linked).unwrap();
    let mut writer = ZipWriter::new(file);
    writer.start_file("manifest.json", options).unwrap();
    writer
        .write_all(&serde_json::to_vec(&archive_manifest).unwrap())
        .unwrap();
    writer
        .add_symlink("Wiki/link", "../outside", options)
        .unwrap();
    writer.finish().unwrap();
    assert_eq!(
        verify_backup(&linked).unwrap_err().code,
        ErrorCode::RestoreFailed
    );
}

#[test]
fn restore_publishes_only_verified_bytes_into_an_empty_target() {
    let (temporary, vault) = setup();
    let archive = temporary.path().join("backup.zip");
    create(&vault, &archive, true);

    let target = temporary.path().join("restored");
    let report = restore_backup(&archive, &target).unwrap();
    assert_eq!(report.target, target);
    assert_eq!(
        fs::read(target.join("Notes/hello.md")).unwrap(),
        b"portable bytes"
    );
    assert!(target.join("Disabled").is_dir());
    assert!(!target.join("manifest.json").exists());
    assert!(!target.join(".kb/cache").exists());

    let nonempty = temporary.path().join("nonempty");
    fs::create_dir(&nonempty).unwrap();
    fs::write(nonempty.join("keep"), "keep").unwrap();
    assert_eq!(
        restore_backup(&archive, &nonempty).unwrap_err().code,
        ErrorCode::TargetNotEmpty
    );
    assert_eq!(fs::read_to_string(nonempty.join("keep")).unwrap(), "keep");

    let empty = temporary.path().join("empty");
    fs::create_dir(&empty).unwrap();
    restore_backup(&archive, &empty).unwrap();
    assert_eq!(
        fs::read_to_string(empty.join("Notes/hello.md")).unwrap(),
        "portable bytes"
    );
}

fn manifest(archive: &Path) -> BackupManifest {
    let mut archive = ZipArchive::new(fs::File::open(archive).unwrap()).unwrap();
    serde_json::from_reader(archive.by_name("manifest.json").unwrap()).unwrap()
}

fn rewrite_archive(source: &Path, target: &Path, replacement_manifest: &BackupManifest) {
    let mut input = ZipArchive::new(fs::File::open(source).unwrap()).unwrap();
    let mut output = ZipWriter::new(fs::File::create(target).unwrap());
    let file_options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    let directory_options =
        SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    for index in 0..input.len() {
        let mut entry = input.by_index(index).unwrap();
        let name = entry.name().to_owned();
        if entry.is_dir() {
            output.add_directory(name, directory_options).unwrap();
        } else {
            output.start_file(&name, file_options).unwrap();
            if name == "manifest.json" {
                output
                    .write_all(&serde_json::to_vec(replacement_manifest).unwrap())
                    .unwrap();
            } else {
                std::io::copy(&mut entry, &mut output).unwrap();
            }
        }
    }
    output.finish().unwrap();
}
