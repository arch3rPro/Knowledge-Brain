use kb_core::{PortableRelativePath, SourceId, SourceVersion};
#[test]
fn identities_escape_uri_delimiters_and_validate_hashes() {
    let id = SourceId::new(
        "notes/#",
        PortableRelativePath::parse("my guide.md").unwrap(),
    )
    .unwrap();
    assert_eq!(id.logical_uri(), "kb-source://notes%2F%23/my%20guide.md");
    assert!(SourceVersion::new(id.clone(), "../bad").is_err());
    let version = SourceVersion::new(id, "a".repeat(64)).unwrap();
    assert!(
        version
            .exact_uri()
            .ends_with(&format!("?sha256={}", "a".repeat(64)))
    );
    assert!(
        serde_json::from_str::<SourceVersion>(
            r#"{"source":{"admission_id":"notes","relative_path":"a.md"},"sha256":"bad"}"#
        )
        .is_err()
    );
}
