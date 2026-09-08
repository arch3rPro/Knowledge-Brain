use kb_app::{
    AppContext, AppRequest, ConfigOverrides, InitRequest, UserPaths, init_vault,
    load_effective_config, query, rebuild_catalog, review_sources, run,
};
use kb_core::{ErrorCode, SearchBackend, SearchMatchMode, SearchMode, SearchRequest, SearchScope};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs, path::Path};

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
        match_mode: SearchMatchMode::Relevant,
    }
}

fn exact_request(query: &str) -> SearchRequest {
    exact_request_with_strict_backend(query, false)
}

fn exact_request_with_strict_backend(query: &str, strict_backend: bool) -> SearchRequest {
    exact_request_for(query, SearchScope::Wiki, 10, strict_backend)
}

fn exact_request_for(
    query: &str,
    scope: SearchScope,
    limit: usize,
    strict_backend: bool,
) -> SearchRequest {
    SearchRequest {
        query: query.into(),
        scope,
        limit,
        strict_backend,
        match_mode: SearchMatchMode::Exact,
    }
}

fn user_paths(base: &Path) -> UserPaths {
    UserPaths::new(base.join("config"), base.join("state"), base.join("cache"))
}

fn app_context(base: &Path) -> AppContext {
    AppContext::new(
        BTreeMap::from([
            (
                "KB_CONFIG_DIR".to_owned(),
                base.join("config").display().to_string(),
            ),
            (
                "KB_STATE_DIR".to_owned(),
                base.join("state").display().to_string(),
            ),
            (
                "KB_CACHE_DIR".to_owned(),
                base.join("cache").display().to_string(),
            ),
        ]),
        base.to_path_buf(),
    )
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

#[test]
fn exact_match_is_case_sensitive_literal_and_ignores_a_stale_bm25_cache() {
    let (_temporary, vault, config) = setup();
    fs::write(
        vault.join("Wiki/articles/exact.md"),
        article("Exact", "kb-incremental-rebuild-probe"),
    )
    .unwrap();
    fs::write(
        vault.join("Wiki/articles/partial.md"),
        article("Partial", "probe"),
    )
    .unwrap();
    rebuild_catalog(&vault, &config).unwrap();
    let cache_path = vault.join(".kb/cache/bm25.json");
    fs::write(&cache_path, "{broken").unwrap();
    let cache_before = fs::read(&cache_path).unwrap();

    let result = query(
        &vault,
        &exact_request("kb-incremental-rebuild-probe"),
        &config,
    )
    .unwrap();
    assert_eq!(result.match_mode, SearchMatchMode::Exact);
    assert!(result.warnings.is_empty());
    assert_eq!(result.groups[0].results.len(), 1);
    assert_eq!(
        result.groups[0].results[0].path.as_str(),
        "Wiki/articles/exact.md"
    );
    assert_eq!(
        result.groups[0].results[0].backend,
        Some(SearchBackend::Direct)
    );
    assert_eq!(result.groups[0].results[0].score_micros, None);
    assert_eq!(result.groups[0].results[0].explanation, None);

    assert!(
        query(
            &vault,
            &exact_request("KB-INCREMENTAL-REBUILD-PROBE"),
            &config,
        )
        .unwrap()
        .groups[0]
            .results
            .is_empty()
    );

    let strict = query(
        &vault,
        &exact_request_with_strict_backend("kb-incremental-rebuild-probe", true),
        &config,
    )
    .unwrap();
    assert!(strict.warnings.is_empty());
    assert_eq!(fs::read(cache_path).unwrap(), cache_before);
}

#[test]
#[allow(clippy::too_many_lines)]
fn exact_all_limits_each_group_preserves_source_metadata_and_does_not_rewrite_bm25() {
    let temporary = tempfile::tempdir().unwrap();
    let vault = temporary.path().join("vault");
    init_vault(&InitRequest {
        target: vault.clone(),
    })
    .unwrap();
    fs::write(
        vault.join("Wiki/articles/wiki-a.md"),
        article(
            "Wiki A",
            "exact-source-needle first\nexact-source-needle second",
        ),
    )
    .unwrap();
    fs::write(
        vault.join("Wiki/articles/wiki-b.md"),
        article("Wiki B", "exact-source-needle once"),
    )
    .unwrap();
    fs::create_dir(vault.join("Notes")).unwrap();
    let source_a = "# Source A\nexact-source-needle first\nexact-source-needle second\n";
    fs::write(vault.join("Notes/a.md"), source_a).unwrap();
    fs::write(
        vault.join("Notes/b.md"),
        "# Source B\nexact-source-needle once\n",
    )
    .unwrap();
    fs::write(
        vault.join("admission.yml"),
        "schema_version: v1.0\ndirectories:\n  - id: notes\n    path: Notes\n    enabled: true\n",
    )
    .unwrap();

    let paths = user_paths(temporary.path());
    let mut config = load_effective_config(&vault, &paths, &ConfigOverrides::default()).unwrap();
    let operation_id = review_sources(&vault, &paths, &config)
        .unwrap()
        .operation_id
        .unwrap();
    run(
        AppRequest::Apply { operation_id },
        &app_context(temporary.path()),
    )
    .unwrap();
    config.search.mode.value = SearchMode::Bm25;
    rebuild_catalog(&vault, &config).unwrap();
    let cache_path = vault.join(".kb/cache/bm25.json");
    let cache_before = fs::read(&cache_path).unwrap();

    let all = query(
        &vault,
        &exact_request_for("exact-source-needle", SearchScope::All, 10, false),
        &config,
    )
    .unwrap();
    assert_eq!(all.match_mode, SearchMatchMode::Exact);
    assert_eq!(all.groups.len(), 2);
    assert_eq!(
        all.groups
            .iter()
            .map(|group| group.scope)
            .collect::<Vec<_>>(),
        vec![SearchScope::Wiki, SearchScope::Sources]
    );
    assert!(all.groups.iter().all(|group| group.results.len() >= 2));

    let limited = query(
        &vault,
        &exact_request_for("exact-source-needle", SearchScope::All, 1, false),
        &config,
    )
    .unwrap();
    assert_eq!(limited.groups.len(), 2);
    assert!(limited.groups.iter().all(|group| group.results.len() == 1));
    let source_hit = &limited
        .groups
        .iter()
        .find(|group| group.scope == SearchScope::Sources)
        .unwrap()
        .results[0];
    let source_sha = format!("{:x}", Sha256::digest(source_a.as_bytes()));
    let source_uri = format!("kb-source://notes/a.md?sha256={source_sha}");
    let record_sha = format!("{:x}", Sha256::digest(b"kb-source://notes/a.md"));
    assert_eq!(
        source_hit.path.as_str(),
        format!(
            "Wiki/external-sources/records/{}/{}.md",
            &record_sha[..2],
            record_sha
        )
    );
    assert_eq!(
        source_hit.content_path.as_str(),
        format!(
            "Wiki/external-sources/.objects/sha256/{}/{}",
            &source_sha[..2],
            source_sha
        )
    );
    assert_eq!(source_hit.source_uri.as_deref(), Some(source_uri.as_str()));
    assert_eq!(source_hit.title, "Source A");
    assert_eq!(source_hit.heading.as_deref(), Some("Source A"));
    assert_eq!(source_hit.line_start, Some(1));
    assert_eq!(source_hit.location, None);
    assert_eq!(source_hit.snippet, "exact-source-needle first");
    assert_eq!(source_hit.match_count, 2);
    assert_eq!(source_hit.backend, Some(SearchBackend::Direct));
    assert_eq!(source_hit.score_micros, None);
    assert_eq!(source_hit.explanation, None);
    assert!(limited.warnings.is_empty());
    assert_eq!(fs::read(cache_path).unwrap(), cache_before);
}

#[test]
fn exact_requests_reuse_query_validation() {
    let (_temporary, vault, config) = setup();
    for (query_text, limit) in [("", 10), ("   ", 10), ("valid", 0), ("valid", 101)] {
        let error = query(
            &vault,
            &exact_request_for(query_text, SearchScope::Wiki, limit, false),
            &config,
        )
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidQuery);
    }
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
