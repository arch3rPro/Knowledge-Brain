use kb_core::{
    CURRENT_SCHEMA_VERSION, KnowledgeChangeRequest, KnowledgePlanRequest, PortableRelativePath,
};

fn change(path: &str) -> KnowledgeChangeRequest {
    KnowledgeChangeRequest {
        path: PortableRelativePath::parse(path).unwrap(),
        before_sha256: None,
        summary: "Add a reusable article.".into(),
        content: "---\ntype: Article\n---\n".into(),
    }
}

#[test]
fn request_accepts_bounded_research_and_article_targets() {
    let request = KnowledgePlanRequest {
        schema_version: CURRENT_SCHEMA_VERSION,
        changes: vec![change("articles/topic.md"), change("research/question.md")],
    };

    request.validate(10, 1024).unwrap();
}

#[test]
fn request_rejects_targets_outside_knowledge_layers() {
    for path in [
        "index.md",
        "log.md",
        "external-sources/record.md",
        "articles/index.md",
        "research/log.md",
        "articles/topic.txt",
    ] {
        let request = KnowledgePlanRequest {
            schema_version: CURRENT_SCHEMA_VERSION,
            changes: vec![change(path)],
        };
        assert!(request.validate(10, 1024).is_err(), "path={path}");
    }
}

#[test]
fn request_rejects_duplicate_paths_invalid_hashes_and_ambiguous_summaries() {
    let duplicate = KnowledgePlanRequest {
        schema_version: CURRENT_SCHEMA_VERSION,
        changes: vec![change("articles/a.md"), change("articles/a.md")],
    };
    assert!(duplicate.validate(10, 1024).is_err());

    let mut invalid_hash = change("articles/a.md");
    invalid_hash.before_sha256 = Some("ABC".into());
    let request = KnowledgePlanRequest {
        schema_version: CURRENT_SCHEMA_VERSION,
        changes: vec![invalid_hash],
    };
    assert!(request.validate(10, 1024).is_err());

    for summary in ["", "  ", "line one\nline two"] {
        let mut invalid_summary = change("articles/a.md");
        invalid_summary.summary = summary.into();
        let request = KnowledgePlanRequest {
            schema_version: CURRENT_SCHEMA_VERSION,
            changes: vec![invalid_summary],
        };
        assert!(request.validate(10, 1024).is_err(), "summary={summary:?}");
    }
}

#[test]
fn request_enforces_schema_count_and_content_size() {
    let empty = KnowledgePlanRequest {
        schema_version: CURRENT_SCHEMA_VERSION,
        changes: Vec::new(),
    };
    assert!(empty.validate(10, 1024).is_err());

    let too_many = KnowledgePlanRequest {
        schema_version: CURRENT_SCHEMA_VERSION,
        changes: vec![change("articles/a.md"), change("articles/b.md")],
    };
    assert!(too_many.validate(1, 1024).is_err());

    let oversized = KnowledgePlanRequest {
        schema_version: CURRENT_SCHEMA_VERSION,
        changes: vec![change("articles/a.md")],
    };
    assert!(oversized.validate(10, 5).is_err());

    let json = r#"{"schema_version":"v1.1","changes":[]}"#;
    let newer: KnowledgePlanRequest = serde_json::from_str(json).unwrap();
    assert!(newer.validate(10, 1024).is_err());
}
