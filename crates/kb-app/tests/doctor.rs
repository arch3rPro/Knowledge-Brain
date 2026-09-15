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

#[test]
fn doctor_reports_template_conflicts_and_incomplete_unified_updates() {
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
    let identity: serde_yaml_ng::Value =
        serde_yaml_ng::from_slice(&std::fs::read(vault.join(".kb/config.yml")).unwrap()).unwrap();
    let vault_id = identity["vault_id"].as_str().unwrap();
    let update = paths.state_dir.join("updates/example");
    std::fs::create_dir_all(&update).unwrap();
    std::fs::write(
        update.join("operation.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "execution_state": "applying_components",
            "scope": { "vaults": [vault_id] }
        }))
        .unwrap(),
    )
    .unwrap();
    let changed = std::fs::read_to_string(vault.join("KB.md"))
        .unwrap()
        .replace("- Use `kb capabilities`", "- Never use `kb capabilities`");
    std::fs::write(vault.join("KB.md"), changed).unwrap();

    let report = doctor(&vault, &paths, &ConfigOverrides::default()).unwrap();

    let template = report
        .checks
        .iter()
        .find(|check| check.id == "vault_template")
        .unwrap();
    assert!(matches!(template.status, CheckStatus::Warn));
    assert!(template.message.contains("KB.md"));
    let updates = report
        .checks
        .iter()
        .find(|check| check.id == "update_operations")
        .unwrap();
    assert!(matches!(updates.status, CheckStatus::Warn));
    assert!(updates.message.contains("1"));
}
