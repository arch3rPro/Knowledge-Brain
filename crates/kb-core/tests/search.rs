use kb_core::{SearchMatchMode, SearchRequest, SearchResponse, SearchScope};

#[test]
fn requests_default_to_relevant_match_mode_and_serialize_exact() {
    assert_eq!(
        serde_json::from_str::<SearchRequest>(
            r#"{"query":"needle","scope":"wiki","limit":10,"strict_backend":false}"#,
        )
        .unwrap()
        .match_mode,
        SearchMatchMode::Relevant,
    );
    assert_eq!(
        serde_json::to_value(SearchMatchMode::Exact).unwrap(),
        serde_json::json!("exact"),
    );
}

#[test]
fn legacy_responses_default_to_relevant_match_mode() {
    let response = serde_json::from_str::<SearchResponse>(
        r#"{"schema_version":"v1.0","query":"needle","groups":[],"warnings":[]}"#,
    )
    .unwrap();

    assert_eq!(response.match_mode, SearchMatchMode::Relevant);
}

#[test]
fn requests_reject_blank_queries_and_invalid_limits() {
    for (q, n) in [("", 10), ("   ", 10), ("valid", 0), ("valid", 101)] {
        assert!(
            SearchRequest {
                query: q.into(),
                scope: SearchScope::Wiki,
                limit: n,
                strict_backend: false,
                match_mode: SearchMatchMode::Relevant,
            }
            .validate()
            .is_err()
        );
    }
    assert!(
        SearchRequest {
            query: "知识".into(),
            scope: SearchScope::All,
            limit: 10,
            strict_backend: false,
            match_mode: SearchMatchMode::Relevant,
        }
        .validate()
        .is_ok()
    );
}
