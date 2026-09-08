use std::fs;

use kb_update::{
    ReplaceRequest, UpdateError, parent_process_start_time, remove_update_stage,
    replace_with_backup, wait_for_parent_exit,
};
use sha2::{Digest, Sha256};

#[test]
fn replacement_rejects_an_invalid_parent_process_id() {
    assert!(matches!(
        wait_for_parent_exit(0, 0),
        Err(UpdateError::ReplacementFailed(_))
    ));
}

#[test]
fn cleanup_removes_only_a_marked_update_stage() {
    let temp = tempfile::tempdir().unwrap();
    let stage = temp.path().join(".kb-update-test");
    fs::create_dir(&stage).unwrap();
    fs::write(stage.join(".cleanup-token"), "expected-token").unwrap();
    fs::write(stage.join("helper"), "old helper").unwrap();

    remove_update_stage(&stage, "expected-token").unwrap();

    assert!(!stage.exists());
}

#[test]
fn cleanup_rejects_an_unmarked_or_mismatched_directory() {
    let temp = tempfile::tempdir().unwrap();
    let ordinary = temp.path().join("ordinary");
    fs::create_dir(&ordinary).unwrap();
    fs::write(ordinary.join("keep"), "important").unwrap();

    assert!(remove_update_stage(&ordinary, "token").is_err());
    assert!(ordinary.join("keep").exists());
}

#[test]
fn replacement_rejects_a_reused_or_mismatched_parent_identity() {
    let pid = std::process::id();
    let started_at = parent_process_start_time(pid).unwrap();

    let error = wait_for_parent_exit(pid, started_at.saturating_add(1)).unwrap_err();

    assert!(error.to_string().contains("identity"));
}

#[test]
fn replacement_restores_backup_when_the_staged_binary_is_missing() {
    let temp = tempfile::tempdir().unwrap();
    let target = temp.path().join("kb");
    let staged = temp.path().join("staged-kb");
    let backup = temp.path().join("kb.backup");
    fs::write(&target, b"old binary").unwrap();

    let request = ReplaceRequest::new(&staged, &target, &backup);
    let error = replace_with_backup(&request).unwrap_err();

    assert!(error.to_string().contains("replace"));
    assert_eq!(fs::read(&target).unwrap(), b"old binary");
    assert!(!backup.exists());
}

#[test]
fn replacement_swaps_verified_binary_and_removes_backup() {
    let temp = tempfile::tempdir().unwrap();
    let target = temp.path().join("kb");
    let staged = temp.path().join("staged-kb");
    let backup = temp.path().join("kb.backup");
    fs::write(&target, b"old binary").unwrap();
    fs::write(&staged, b"new binary").unwrap();

    let request = ReplaceRequest::new(&staged, &target, &backup);
    replace_with_backup(&request).unwrap();

    assert_eq!(fs::read(&target).unwrap(), b"new binary");
    assert!(!backup.exists());
}

#[test]
fn replacement_restores_backup_when_final_binary_digest_is_wrong() {
    let temp = tempfile::tempdir().unwrap();
    let target = temp.path().join("kb");
    let staged = temp.path().join("staged-kb");
    let backup = temp.path().join("kb.backup");
    fs::write(&target, b"old binary").unwrap();
    fs::write(&staged, b"tampered binary").unwrap();
    let expected = hex::encode(Sha256::digest(b"expected binary"));
    let request = ReplaceRequest::verified(&staged, &target, &backup, expected);

    let error = replace_with_backup(&request).unwrap_err();

    assert!(matches!(error, UpdateError::ReplacementFailed(_)));
    assert_eq!(fs::read(&target).unwrap(), b"old binary");
    assert!(!backup.exists());
}
