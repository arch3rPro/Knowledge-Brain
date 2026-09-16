use std::fs;

use kb_app::{ConfigOverrides, InitRequest, UserPaths, init_vault, load_effective_config, query};
use kb_core::{ResourcePathScope, SearchMatchMode, SearchRequest, SearchScope};

#[test]
fn direct_wiki_search_returns_a_server_scoped_readable_resource() {
    let temporary = tempfile::tempdir().unwrap();
    let vault = temporary.path().join("vault");
    let initialized = init_vault(&InitRequest {
        target: vault.clone(),
    })
    .unwrap();
    fs::write(
        vault.join("Wiki/articles/resource.md"),
        "# Resource\n\nremote-readable-needle\n",
    )
    .unwrap();
    let paths = UserPaths::new(
        temporary.path().join("config"),
        temporary.path().join("state"),
        temporary.path().join("cache"),
    );
    let config = load_effective_config(&vault, &paths, &ConfigOverrides::default()).unwrap();

    let response = query(
        &vault,
        &SearchRequest {
            query: "remote-readable-needle".into(),
            scope: SearchScope::Wiki,
            limit: 10,
            strict_backend: false,
            match_mode: SearchMatchMode::Exact,
        },
        &config,
    )
    .unwrap();
    let hit = &response.groups[0].results[0];
    assert_eq!(
        hit.resource_uri.as_deref(),
        Some(
            format!(
                "kb-vault://{}/Wiki/articles/resource.md",
                initialized.vault_id
            )
            .as_str()
        )
    );
    assert_eq!(hit.path_scope, Some(ResourcePathScope::ServerVault));
}
