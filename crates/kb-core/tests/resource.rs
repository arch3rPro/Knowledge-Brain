use kb_core::{KnowledgeResourceUri, ResourceReadRequest};
use uuid::Uuid;

#[test]
fn parses_vault_rules_and_nested_wiki_resources() {
    let vault_id = Uuid::parse_str("123e4567-e89b-12d3-a456-426614174000").unwrap();

    let rules = format!("kb-vault://{vault_id}/KB.md")
        .parse::<KnowledgeResourceUri>()
        .unwrap();
    assert_eq!(rules.to_string(), format!("kb-vault://{vault_id}/KB.md"));

    let article = format!("kb-vault://{vault_id}/Wiki/articles/nested/example.md")
        .parse::<KnowledgeResourceUri>()
        .unwrap();
    assert_eq!(
        article.to_string(),
        format!("kb-vault://{vault_id}/Wiki/articles/nested/example.md")
    );
}

#[test]
fn parses_exact_percent_encoded_source_resource() {
    let sha = "a".repeat(64);
    let uri = format!("kb-source://team%20notes/folder/file%20name.md?sha256={sha}");
    let parsed = uri.parse::<KnowledgeResourceUri>().unwrap();

    assert_eq!(parsed.to_string(), uri);
}

#[test]
fn rejects_unsafe_or_incomplete_resource_uris() {
    let vault_id = "123e4567-e89b-12d3-a456-426614174000";
    let invalid = [
        format!("kb-vault://{vault_id}/../secret.md"),
        format!("kb-vault://{vault_id}/Wiki/article.txt"),
        format!("kb-vault://{vault_id}/Wiki/log.md"),
        format!("kb-vault://{vault_id}/Wiki/external-sources/records/aa/source.md"),
        "kb-vault://not-a-uuid/KB.md".to_owned(),
        "kb-source://notes/file.md".to_owned(),
        "kb-source://notes/file.md?sha256=abc".to_owned(),
        "file:///tmp/KB.md".to_owned(),
    ];

    for uri in invalid {
        assert!(
            uri.parse::<KnowledgeResourceUri>().is_err(),
            "unexpectedly accepted {uri}"
        );
    }
}

#[test]
fn validates_read_page_size() {
    let valid = ResourceReadRequest {
        resource_uri: "kb-vault://123e4567-e89b-12d3-a456-426614174000/KB.md".into(),
        cursor: None,
        max_chars: 16_000,
    };
    valid.validate().unwrap();

    for max_chars in [0, 1_000_001] {
        let invalid = ResourceReadRequest {
            max_chars,
            ..valid.clone()
        };
        assert!(invalid.validate().is_err());
    }
}
