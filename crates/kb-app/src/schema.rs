use kb_core::{CURRENT_SCHEMA_VERSION, MigrationCatalog, SchemaCompatibility, SchemaVersion};

#[allow(
    dead_code,
    reason = "The application schema policy will consume this catalog as migration support is added."
)]
fn production_migration_catalog() -> MigrationCatalog {
    MigrationCatalog::empty()
}

#[allow(
    dead_code,
    reason = "Application schema policy callers are added independently of this fixed catalog."
)]
pub(crate) fn vault_schema_compatibility(version: SchemaVersion) -> SchemaCompatibility {
    production_migration_catalog().classify(version, CURRENT_SCHEMA_VERSION)
}

#[cfg(test)]
mod tests {
    use kb_core::{CURRENT_SCHEMA_VERSION, SchemaCompatibility, SchemaVersion};

    use super::vault_schema_compatibility;

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
}
