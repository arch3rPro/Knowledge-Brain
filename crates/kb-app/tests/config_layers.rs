use std::collections::BTreeMap;

use kb_app::{ConfigOverrides, UserPaths, load_effective_config};
use kb_core::ConfigSource;

#[test]
fn precedence_is_cli_env_local_vault_user_default() {
    let temp = tempfile::tempdir().unwrap();
    let vault = temp.path().join("vault");
    let user_config = temp.path().join("user-config");
    std::fs::create_dir_all(vault.join(".kb")).unwrap();
    std::fs::create_dir_all(&user_config).unwrap();
    std::fs::write(
        user_config.join("config.yml"),
        "schema_version: \"v1.0\"\nlimits:\n  max_file_bytes: 10\n",
    )
    .unwrap();
    std::fs::write(
        vault.join(".kb/config.yml"),
        "schema_version: \"v1.0\"\nvault_id: \"20e4d3b4-5f9a-4d61-8ed0-c8f86bbda352\"\nlimits:\n  max_file_bytes: 20\n",
    )
    .unwrap();
    std::fs::write(
        vault.join(".kb/config.local.yml"),
        "schema_version: \"v1.0\"\nlimits:\n  max_file_bytes: 30\n",
    )
    .unwrap();

    let user_paths = UserPaths::new(
        user_config,
        temp.path().join("state"),
        temp.path().join("cache"),
    );
    let mut overrides = ConfigOverrides {
        environment: BTreeMap::from([("KB_LIMITS_MAX_FILE_BYTES".to_owned(), "40".to_owned())]),
        cli: BTreeMap::from([("limits.max_file_bytes".to_owned(), "50".to_owned())]),
    };

    let loaded = load_effective_config(&vault, &user_paths, &overrides).unwrap();
    assert_eq!(loaded.limits.max_file_bytes.value, 50);
    assert_eq!(loaded.limits.max_file_bytes.source, ConfigSource::Cli);
    assert_eq!(loaded.search.mode.source, ConfigSource::BuiltIn);
    assert!(!loaded.files.include_hidden.value);
    assert_eq!(loaded.operations.plan_retention_hours.value, 168);

    overrides.cli.clear();
    let loaded = load_effective_config(&vault, &user_paths, &overrides).unwrap();
    assert_eq!(loaded.limits.max_file_bytes.value, 40);
    assert_eq!(
        loaded.limits.max_file_bytes.source,
        ConfigSource::Environment
    );

    overrides.environment.clear();
    let loaded = load_effective_config(&vault, &user_paths, &overrides).unwrap();
    assert_eq!(loaded.limits.max_file_bytes.value, 30);
    assert_eq!(
        loaded.limits.max_file_bytes.source,
        ConfigSource::VaultLocal
    );

    std::fs::remove_file(vault.join(".kb/config.local.yml")).unwrap();
    let loaded = load_effective_config(&vault, &user_paths, &overrides).unwrap();
    assert_eq!(loaded.limits.max_file_bytes.value, 20);
    assert_eq!(loaded.limits.max_file_bytes.source, ConfigSource::Vault);

    std::fs::write(
        vault.join(".kb/config.yml"),
        "schema_version: \"v1.0\"\nvault_id: \"20e4d3b4-5f9a-4d61-8ed0-c8f86bbda352\"\n",
    )
    .unwrap();
    let loaded = load_effective_config(&vault, &user_paths, &overrides).unwrap();
    assert_eq!(loaded.limits.max_file_bytes.value, 10);
    assert_eq!(loaded.limits.max_file_bytes.source, ConfigSource::User);

    std::fs::remove_file(user_paths.config_dir.join("config.yml")).unwrap();
    let loaded = load_effective_config(&vault, &user_paths, &overrides).unwrap();
    assert_eq!(loaded.limits.max_file_bytes.value, 50 * 1024 * 1024);
    assert_eq!(loaded.limits.max_file_bytes.source, ConfigSource::BuiltIn);
}

#[test]
fn invalid_environment_value_does_not_fall_through() {
    let temp = tempfile::tempdir().unwrap();
    let vault = temp.path().join("vault");
    std::fs::create_dir_all(vault.join(".kb")).unwrap();
    std::fs::write(
        vault.join(".kb/config.yml"),
        "schema_version: \"v1.0\"\nvault_id: \"20e4d3b4-5f9a-4d61-8ed0-c8f86bbda352\"\nlimits:\n  max_file_bytes: 20\nunknown: keep\n",
    )
    .unwrap();
    let user_paths = UserPaths::new(
        temp.path().join("config"),
        temp.path().join("state"),
        temp.path().join("cache"),
    );
    let overrides = ConfigOverrides {
        environment: BTreeMap::from([(
            "KB_LIMITS_MAX_FILE_BYTES".to_owned(),
            "not-a-number".to_owned(),
        )]),
        cli: BTreeMap::new(),
    };

    let error = load_effective_config(&vault, &user_paths, &overrides).unwrap_err();
    assert_eq!(error.code, kb_core::ErrorCode::InvalidConfig);
    assert!(error.message.contains("KB_LIMITS_MAX_FILE_BYTES"));
}

#[test]
fn empty_user_directory_override_is_rejected() {
    let environment = BTreeMap::from([("KB_CONFIG_DIR".to_owned(), "  ".to_owned())]);
    let error = UserPaths::resolve(&environment).unwrap_err();
    assert_eq!(error.code, kb_core::ErrorCode::InvalidConfig);
    assert!(error.message.contains("KB_CONFIG_DIR"));
}
