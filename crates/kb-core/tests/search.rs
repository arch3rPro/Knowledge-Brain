use kb_core::{SearchRequest, SearchScope};
#[test]
fn requests_reject_blank_queries_and_invalid_limits() {
    for (q, n) in [("", 10), ("   ", 10), ("valid", 0), ("valid", 101)] {
        assert!(
            SearchRequest {
                query: q.into(),
                scope: SearchScope::Wiki,
                limit: n,
                strict_backend: false,
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
        }
        .validate()
        .is_ok()
    );
}
