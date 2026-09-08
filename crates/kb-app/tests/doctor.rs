use kb_app::{CheckStatus, ConfigOverrides, InitRequest, UserPaths, doctor, init_vault};

#[test]
fn doctor_separates_vault_structure_from_on_demand_machine_runtime_directories() {
    let temporary = tempfile::tempdir().unwrap();
    let vault = temporary.path().join("vault");
    init_vault(&InitRequest {
        target: vault.clone(),
    })
    .unwrap();
    let unused_user_paths = UserPaths::new(
        temporary.path().join("config"),
        temporary.path().join("state"),
        temporary.path().join("cache"),
    );

    let report = doctor(&vault, &unused_user_paths, &ConfigOverrides::default()).unwrap();
    let runtime = report
        .checks
        .iter()
        .find(|check| check.id == "machine_runtime_directories")
        .unwrap();

    assert!(matches!(runtime.status, CheckStatus::Pass));
    assert!(runtime.message.contains("created on demand"));
    assert!(
        report
            .checks
            .iter()
            .any(|check| check.id == "vault_structure")
    );
}
