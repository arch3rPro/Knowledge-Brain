use std::{collections::BTreeMap, fs};

use kb_app::{
    AppContext, AppRequest, ConfigOverrides, InitRequest, SaveMode, UserPaths, init_vault,
    load_effective_config, read_resource, run,
};
use kb_core::{ErrorCode, ResourceKind, ResourceReadRequest};

fn setup() -> (tempfile::TempDir, std::path::PathBuf, UserPaths, uuid::Uuid) {
    let temporary = tempfile::tempdir().unwrap();
    let vault = temporary.path().join("vault");
    let report = init_vault(&InitRequest {
        target: vault.clone(),
    })
    .unwrap();
    let paths = UserPaths::new(
        temporary.path().join("config"),
        temporary.path().join("state"),
        temporary.path().join("cache"),
    );
    (temporary, vault, paths, report.vault_id)
}

#[test]
fn reads_rules_and_wiki_with_normalized_links() {
    let (_temporary, vault, paths, vault_id) = setup();
    fs::create_dir_all(vault.join("Wiki/articles/nested")).unwrap();
    fs::write(
        vault.join("Wiki/articles/related.md"),
        "# Related\n\nEvidence.\n",
    )
    .unwrap();
    fs::write(
        vault.join("Wiki/articles/nested/example.md"),
        "# Example\n\n[Related](../related.md) and [Web](https://example.com).\n",
    )
    .unwrap();
    let config = load_effective_config(&vault, &paths, &ConfigOverrides::default()).unwrap();

    let rules = read_resource(
        &vault,
        &config,
        &ResourceReadRequest {
            resource_uri: format!("kb-vault://{vault_id}/KB.md"),
            cursor: None,
            max_chars: 16_000,
        },
    )
    .unwrap();
    assert_eq!(rules.kind, ResourceKind::Rules);
    assert!(rules.content.unwrap().contains("Knowledge-Brain"));

    let article = read_resource(
        &vault,
        &config,
        &ResourceReadRequest {
            resource_uri: format!("kb-vault://{vault_id}/Wiki/articles/nested/example.md"),
            cursor: None,
            max_chars: 16_000,
        },
    )
    .unwrap();
    assert_eq!(article.kind, ResourceKind::Wiki);
    assert_eq!(article.links.len(), 2);
    assert_eq!(
        article.links[0].target_uri,
        format!("kb-vault://{vault_id}/Wiki/articles/related.md")
    );
    assert_eq!(article.links[0].exists, Some(true));
    assert!(article.links[1].external);
}

#[test]
fn paginates_unicode_and_rejects_a_stale_cursor() {
    let (_temporary, vault, paths, vault_id) = setup();
    let path = vault.join("Wiki/articles/long.md");
    fs::write(&path, "一二三四五六七八九十").unwrap();
    let config = load_effective_config(&vault, &paths, &ConfigOverrides::default()).unwrap();
    let uri = format!("kb-vault://{vault_id}/Wiki/articles/long.md");

    let first = read_resource(
        &vault,
        &config,
        &ResourceReadRequest {
            resource_uri: uri.clone(),
            cursor: None,
            max_chars: 4,
        },
    )
    .unwrap();
    assert_eq!(first.content.as_deref(), Some("一二三四"));
    assert!(!first.complete);

    let cursor = first.next_cursor.unwrap();
    let second = read_resource(
        &vault,
        &config,
        &ResourceReadRequest {
            resource_uri: uri.clone(),
            cursor: Some(cursor.clone()),
            max_chars: 4,
        },
    )
    .unwrap();
    assert_eq!(second.content.as_deref(), Some("五六七八"));

    fs::write(path, "内容已经变化").unwrap();
    let stale = read_resource(
        &vault,
        &config,
        &ResourceReadRequest {
            resource_uri: uri,
            cursor: Some(cursor),
            max_chars: 4,
        },
    )
    .unwrap_err();
    assert_eq!(stale.code, ErrorCode::ResourceCursorStale);
}

#[test]
fn rejects_wrong_vault_and_nonexistent_resources() {
    let (_temporary, vault, paths, vault_id) = setup();
    let config = load_effective_config(&vault, &paths, &ConfigOverrides::default()).unwrap();

    let wrong = read_resource(
        &vault,
        &config,
        &ResourceReadRequest {
            resource_uri: format!("kb-vault://{}/KB.md", uuid::Uuid::new_v4()),
            cursor: None,
            max_chars: 100,
        },
    )
    .unwrap_err();
    assert_eq!(wrong.code, ErrorCode::ResourceOutOfScope);

    let missing = read_resource(
        &vault,
        &config,
        &ResourceReadRequest {
            resource_uri: format!("kb-vault://{vault_id}/Wiki/articles/missing.md"),
            cursor: None,
            max_chars: 100,
        },
    )
    .unwrap_err();
    assert_eq!(missing.code, ErrorCode::ResourceNotFound);
}

#[test]
fn reads_only_exact_saved_source_versions_and_reports_unavailable_pdf_text() {
    let (temporary, vault, paths, _vault_id) = setup();
    fs::create_dir(vault.join("Notes")).unwrap();
    fs::write(
        vault.join("Notes/evidence.md"),
        "# Evidence\n\nSaved text.\n",
    )
    .unwrap();
    fs::write(vault.join("Notes/attachment.pdf"), b"%PDF-1.4 fixture").unwrap();
    fs::write(
        vault.join("admission.yml"),
        "schema_version: v1.0\ndirectories:\n  - id: notes\n    path: Notes\n    enabled: true\n",
    )
    .unwrap();
    let context = AppContext::new(
        BTreeMap::from([
            (
                "KB_CONFIG_DIR".to_owned(),
                temporary.path().join("config").display().to_string(),
            ),
            (
                "KB_STATE_DIR".to_owned(),
                temporary.path().join("state").display().to_string(),
            ),
            (
                "KB_CACHE_DIR".to_owned(),
                temporary.path().join("cache").display().to_string(),
            ),
        ]),
        temporary.path().to_path_buf(),
    );
    let selector = vault.display().to_string();
    let prepared = run(
        AppRequest::SourceSave {
            vault: Some(selector.clone()),
            mode: SaveMode::Prepare,
        },
        &context,
    )
    .unwrap();
    let token = prepared["confirmation_token"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let applied = run(
        AppRequest::SourceSave {
            vault: Some(selector),
            mode: SaveMode::Confirm(token),
        },
        &context,
    )
    .unwrap();
    let captured = applied["result"]["captured"].as_array().unwrap();
    let markdown_uri = captured
        .iter()
        .filter_map(|value| value.as_str())
        .find(|uri| uri.contains("evidence.md"))
        .unwrap();
    let pdf_uri = captured
        .iter()
        .filter_map(|value| value.as_str())
        .find(|uri| uri.contains("attachment.pdf"))
        .unwrap();
    let config = load_effective_config(&vault, &paths, &ConfigOverrides::default()).unwrap();

    let markdown = read_resource(
        &vault,
        &config,
        &ResourceReadRequest {
            resource_uri: markdown_uri.into(),
            cursor: None,
            max_chars: 16_000,
        },
    )
    .unwrap();
    assert_eq!(markdown.kind, ResourceKind::Source);
    assert_eq!(
        markdown.content.as_deref(),
        Some("# Evidence\n\nSaved text.\n")
    );

    let pdf = read_resource(
        &vault,
        &config,
        &ResourceReadRequest {
            resource_uri: pdf_uri.into(),
            cursor: None,
            max_chars: 16_000,
        },
    )
    .unwrap();
    assert!(!pdf.text_available);
    assert!(pdf.content.is_none());
    assert!(pdf.warnings.iter().any(|warning| warning.contains("PDF")));
}
