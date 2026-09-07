use kb_core::{
    OkfDocumentKind, OkfSeverity, PortableRelativePath, parse_okf, validate_okf,
};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

fn parse(path: &str, text: &str) -> kb_core::ParsedOkfDocument {
    parse_okf(PortableRelativePath::parse(path).unwrap(), text)
}

fn now() -> OffsetDateTime {
    OffsetDateTime::parse("2026-09-07T12:00:00+08:00", &Rfc3339).unwrap()
}

fn codes(document: &kb_core::ParsedOkfDocument) -> Vec<String> {
    validate_okf(document, now())
        .iter()
        .map(|finding| finding.code.clone())
        .collect()
}

#[test]
fn ordinary_concepts_require_only_valid_frontmatter_and_type() {
    let valid = parse(
        "Wiki/articles/open.md",
        "---\ntype: Personal Note\nunknown_extension: keep\n---\n\nBody\n",
    );
    assert!(validate_okf(&valid, now()).is_empty());

    let missing = parse("Wiki/articles/missing.md", "# No frontmatter\n");
    assert_eq!(codes(&missing), ["frontmatter_required"]);

    let empty_type = parse(
        "Wiki/articles/type.md",
        "---\ntype: '  '\n---\n\nBody\n",
    );
    assert_eq!(codes(&empty_type), ["type_required"]);
}

#[test]
fn managed_profile_requires_explicit_provenance_fields() {
    let document = parse(
        "Wiki/research/managed.md",
        "---\ntype: Research\nkb:\n  managed: true\n---\n\nBody\n",
    );
    assert_eq!(
        codes(&document),
        [
            "managed_generated_required",
            "managed_sources_required",
            "managed_status_required",
            "managed_title_required",
        ]
    );

    let valid = parse(
        "Wiki/articles/managed.md",
        "---\ntype: Article\ntitle: Portable knowledge\nstatus: stable\ngenerated:\n  by: process:knowledge-brain/0.1.0\n  at: 2026-09-07T03:00:00Z\nsources:\n  - id: source-a\n    resource: https://example.com/a\nkb:\n  managed: true\n---\n\nBody\n",
    );
    assert!(validate_okf(&valid, now()).is_empty());
}

#[test]
fn present_optional_families_are_validated_without_equating_stable_and_verified() {
    let document = parse(
        "Wiki/articles/lifecycle.md",
        "---\ntype: Article\nstatus: unknown\ngenerated: true\nverified:\n  by: ''\n  at: yesterday\nstale_after: 2026-09-07T03:59:59Z\nsources:\n  - id: duplicate\n    resource: ''\n  - id: duplicate\n    resource: second\n---\n\nBody\n",
    );
    let findings = validate_okf(&document, now());
    assert_eq!(
        findings
            .iter()
            .map(|finding| (finding.severity, finding.code.as_str()))
            .collect::<Vec<_>>(),
        [
            (OkfSeverity::Error, "status_invalid"),
            (OkfSeverity::Error, "generated_invalid"),
            (OkfSeverity::Error, "verified_actor_required"),
            (OkfSeverity::Error, "verified_timestamp_invalid"),
            (OkfSeverity::Warning, "stale_document"),
            (OkfSeverity::Error, "source_id_duplicate"),
            (OkfSeverity::Error, "source_resource_required"),
        ]
    );

    let stable = parse(
        "Wiki/articles/stable.md",
        "---\ntype: Article\nstatus: stable\n---\n",
    );
    assert!(!codes(&stable).iter().any(|code| code == "verified_required"));
}

#[test]
fn markdown_links_ignore_code_and_preserve_source_lines() {
    let document = parse(
        "Wiki/articles/links.md",
        "---\ntype: Article\n---\n\nSee [real](../research/real.md).\n\n```md\n[ignored](missing.md)\n```\n",
    );
    assert_eq!(document.links.len(), 1);
    assert_eq!(document.links[0].destination, "../research/real.md");
    assert_eq!(document.links[0].line, 5);
}

#[test]
fn reserved_files_follow_their_own_okf_rules() {
    let index = parse(
        "Wiki/index.md",
        "---\nokf_version: '0.3'\n---\n\n# Index\n",
    );
    assert_eq!(index.kind, OkfDocumentKind::Index);
    assert_eq!(codes(&index), ["okf_version_unsupported"]);

    let nested = parse(
        "Wiki/research/index.md",
        "---\nokf_version: '0.2'\n---\n\n# Index\n",
    );
    assert_eq!(codes(&nested), ["reserved_frontmatter_forbidden"]);

    let log = parse(
        "Wiki/log.md",
        "# Log\n\n## September 7\nentry\n\n## 2026-09-06\nentry\n",
    );
    assert_eq!(log.kind, OkfDocumentKind::Log);
    assert_eq!(codes(&log), ["log_date_heading_invalid"]);
}

#[test]
fn supersedes_is_exposed_for_vault_level_resolution() {
    let document = parse(
        "Wiki/articles/new.md",
        "---\ntype: Article\nkb:\n  supersedes:\n    - articles/old.md\n    - 42\n---\n",
    );
    assert_eq!(document.supersedes, ["articles/old.md"]);
    assert_eq!(codes(&document), ["supersedes_entry_invalid"]);
}
