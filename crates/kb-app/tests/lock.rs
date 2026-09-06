use kb_app::{InitRequest, LockMode, VaultLock, init_vault};
use kb_core::ErrorCode;

#[test]
fn incompatible_vault_lock_is_nonblocking_and_reports_owner() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("vault");
    init_vault(&InitRequest {
        target: root.clone(),
    })
    .unwrap();

    let first = VaultLock::acquire(&root, LockMode::Exclusive, "first writer", None).unwrap();
    let error = VaultLock::acquire(&root, LockMode::Exclusive, "second writer", None)
        .err()
        .expect("second exclusive lock must not block or succeed");
    assert_eq!(error.code, ErrorCode::WriteBusy);
    assert!(root.join(".kb/runtime/vault-lock-info.json").is_file());

    drop(first);
    assert!(!root.join(".kb/runtime/vault-lock-info.json").exists());
    VaultLock::acquire(&root, LockMode::Exclusive, "next writer", None).unwrap();
}

#[test]
fn multiple_readers_can_share_a_vault_lock() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("vault");
    init_vault(&InitRequest {
        target: root.clone(),
    })
    .unwrap();

    let _first = VaultLock::acquire(&root, LockMode::Shared, "reader one", None).unwrap();
    let _second = VaultLock::acquire(&root, LockMode::Shared, "reader two", None).unwrap();
}
