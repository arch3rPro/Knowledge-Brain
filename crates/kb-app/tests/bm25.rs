use kb_app::{
    ConfigOverrides, InitRequest, UserPaths, init_vault, load_effective_config, query,
    rebuild_catalog,
};
use kb_core::{ErrorCode, SearchBackend, SearchMode, SearchRequest, SearchScope};
use std::{fs, path::Path};

fn setup() -> (
    tempfile::TempDir,
    std::path::PathBuf,
    kb_core::EffectiveConfig,
) {
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
    let mut config = load_effective_config(&vault, &paths, &ConfigOverrides::default()).unwrap();
    config.search.mode.value = SearchMode::Bm25;
    (temporary, vault, config)
}

fn request(query: &str, strict_backend: bool) -> SearchRequest {
    SearchRequest {
        query: query.into(),
        scope: SearchScope::Wiki,
        limit: 10,
        strict_backend,
    }
}

fn article(title: &str, body: &str) -> String {
    format!(
        "---\ntype: Article\ntitle: {title}\naliases: [备用名称]\ntags: [检索]\n---\n\n# Section\n{body}\n"
    )
}

#[test]
fn bm25f_boosts_fields_supports_cjk_and_explains_scores() {
    let (_temporary, vault, config) = setup();
    fs::write(
        vault.join("Wiki/articles/title.md"),
        article("Needle", "unrelated"),
    )
    .unwrap();
    fs::write(
        vault.join("Wiki/articles/body.md"),
        article("Other", "needle"),
    )
    .unwrap();
    rebuild_catalog(&vault, &config).unwrap();

    let result = query(&vault, &request("needle", false), &config).unwrap();
    assert_eq!(
        result.groups[0].results[0].path.as_str(),
        "Wiki/articles/title.md"
    );
    let hit = &result.groups[0].results[0];
    assert_eq!(hit.backend, Some(SearchBackend::Bm25f));
    assert!(hit.score_micros.unwrap() > 0);
    assert!(
        hit.explanation
            .as_ref()
            .unwrap()
            .fields
            .iter()
            .any(|field| field.field == kb_core::SearchField::Title)
    );

    let cjk = query(&vault, &request("备用名称", false), &config).unwrap();
    assert!(!cjk.groups[0].results.is_empty());
    assert!(
        cjk.groups[0].results[0]
            .explanation
            .as_ref()
            .unwrap()
            .query_terms
            .contains(&"备用名".into())
    );
}

#[test]
fn index_updates_add_change_delete_and_reuses_unchanged_documents() {
    let (_temporary, vault, config) = setup();
    let a = vault.join("Wiki/articles/a.md");
    let b = vault.join("Wiki/articles/b.md");
    fs::write(&a, article("A", "alpha")).unwrap();
    fs::write(&b, article("B", "beta")).unwrap();
    rebuild_catalog(&vault, &config).unwrap();
    let first = index(&vault);
    let a_generation = generation(&first, "Wiki/articles/a.md");
    let b_generation = generation(&first, "Wiki/articles/b.md");

    fs::write(&b, article("B", "beta changed")).unwrap();
    let c = vault.join("Wiki/articles/c.md");
    fs::write(&c, article("C", "gamma")).unwrap();
    rebuild_catalog(&vault, &config).unwrap();
    let second = index(&vault);
    assert_eq!(generation(&second, "Wiki/articles/a.md"), a_generation);
    assert!(generation(&second, "Wiki/articles/b.md") > b_generation);
    assert!(generation(&second, "Wiki/articles/c.md") > 0);

    fs::remove_file(&b).unwrap();
    rebuild_catalog(&vault, &config).unwrap();
    let third = index(&vault);
    assert!(
        third["documents"]
            .as_array()
            .unwrap()
            .iter()
            .all(|document| document["path"] != "Wiki/articles/b.md")
    );
}

#[test]
fn stale_or_corrupt_index_falls_back_unless_strict() {
    let (_temporary, vault, config) = setup();
    let target = vault.join("Wiki/articles/a.md");
    fs::write(&target, article("A", "first")).unwrap();
    rebuild_catalog(&vault, &config).unwrap();
    fs::write(&target, article("A", "fresh direct text")).unwrap();

    let fallback = query(&vault, &request("fresh direct text", false), &config).unwrap();
    assert!(!fallback.warnings.is_empty());
    assert_eq!(
        fallback.groups[0].results[0].backend,
        Some(SearchBackend::Direct)
    );
    let error = query(&vault, &request("fresh direct text", true), &config).unwrap_err();
    assert_eq!(error.code, ErrorCode::IndexStale);

    fs::write(vault.join(".kb/cache/bm25.json"), "{broken").unwrap();
    assert_eq!(
        query(&vault, &request("fresh", true), &config)
            .unwrap_err()
            .code,
        ErrorCode::IndexStale
    );

    let mut structurally_invalid = index_after_rebuild(&vault, &config);
    structurally_invalid["documents"][0]["chunks"][0]["fields"] = serde_json::json!([]);
    fs::write(
        vault.join(".kb/cache/bm25.json"),
        serde_json::to_vec_pretty(&structurally_invalid).unwrap(),
    )
    .unwrap();
    assert_eq!(
        query(&vault, &request("first", true), &config)
            .unwrap_err()
            .code,
        ErrorCode::IndexStale
    );
}

fn index(vault: &Path) -> serde_json::Value {
    serde_json::from_slice(&fs::read(vault.join(".kb/cache/bm25.json")).unwrap()).unwrap()
}

fn generation(index: &serde_json::Value, path: &str) -> u64 {
    index["documents"]
        .as_array()
        .unwrap()
        .iter()
        .find(|document| document["path"] == path)
        .unwrap()["generation"]
        .as_u64()
        .unwrap()
}

fn index_after_rebuild(vault: &Path, config: &kb_core::EffectiveConfig) -> serde_json::Value {
    rebuild_catalog(vault, config).unwrap();
    index(vault)
}
