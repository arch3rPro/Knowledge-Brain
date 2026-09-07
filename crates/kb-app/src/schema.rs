use kb_core::{CURRENT_SCHEMA_VERSION, MigrationCatalog, SchemaCompatibility, SchemaVersion};

fn production_migration_catalog() -> MigrationCatalog {
    MigrationCatalog::empty()
}

pub(crate) fn vault_schema_compatibility(version: SchemaVersion) -> SchemaCompatibility {
    production_migration_catalog().classify(version, CURRENT_SCHEMA_VERSION)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use kb_core::{CURRENT_SCHEMA_VERSION, ErrorCode, SchemaCompatibility, SchemaVersion};

    use super::vault_schema_compatibility;
    use crate::{
        ConfigOverrides, InitRequest, UserPaths, ValidationState, doctor, init_vault,
        load_effective_config, vault_status,
    };

    fn unsupported_schema_vault() -> (tempfile::TempDir, std::path::PathBuf, UserPaths) {
        let temporary = tempfile::tempdir().unwrap();
        let vault = temporary.path().join("vault");
        init_vault(&InitRequest {
            target: vault.clone(),
        })
        .unwrap();
        let config_path = vault.join(".kb/config.yml");
        let config = fs::read_to_string(&config_path).unwrap();
        fs::write(&config_path, config.replacen("v1.0", "v0.9", 1)).unwrap();
        let paths = UserPaths::new(
            temporary.path().join("config"),
            temporary.path().join("state"),
            temporary.path().join("cache"),
        );
        (temporary, vault, paths)
    }

    #[test]
    fn production_catalog_does_not_invent_legacy_migrations() {
        assert_eq!(
            vault_schema_compatibility(SchemaVersion::new(0, 9)),
            SchemaCompatibility::OlderUnsupported,
        );
        assert_eq!(
            vault_schema_compatibility(CURRENT_SCHEMA_VERSION),
            SchemaCompatibility::Current,
        );
    }

    #[test]
    fn schema_migration_status_does_not_interpret_unsupported_vault_configuration() {
        let (_temporary, vault, paths) = unsupported_schema_vault();

        let status = vault_status(&vault, &paths, &ConfigOverrides::default()).unwrap();

        assert_eq!(
            status.schema.compatibility,
            SchemaCompatibility::OlderUnsupported
        );
        assert!(matches!(
            status.configuration,
            ValidationState::NotInterpreted
        ));
        assert_eq!(status.admission.enabled, None);
    }

    #[test]
    fn schema_migration_rejects_mutations_without_a_migration_path() {
        let (_temporary, vault, _paths) = unsupported_schema_vault();

        let error = crate::app::ensure_mutation_allowed(&vault).unwrap_err();

        assert_eq!(error.code, ErrorCode::MigrationUnavailable);
        assert!(error.message.contains("v0.9"));
    }

    #[test]
    fn schema_migration_doctor_warns_when_no_migration_path_exists() {
        let (_temporary, vault, paths) = unsupported_schema_vault();

        let report = doctor(&vault, &paths, &ConfigOverrides::default()).unwrap();

        assert!(report.checks.iter().any(|check| {
            check.id == "configuration" && check.message.contains("no migration path")
        }));
    }

    #[test]
    fn schema_migration_config_loader_rejects_unsupported_vault_schema() {
        let (_temporary, vault, paths) = unsupported_schema_vault();

        let error = load_effective_config(&vault, &paths, &ConfigOverrides::default()).unwrap_err();

        assert_eq!(error.code, ErrorCode::InvalidConfig);
    }
}
