use kb_app::{classify_media_type, extract_bytes};
use kb_core::{
    ExtractedBlock, ExtractedDocument, ExtractedLink, ExtractionStatus, MediaType, SourceLocation,
};
use std::path::Path;

#[test]
fn document_extensions_have_distinct_media_types() {
    assert_eq!(classify_media_type(Path::new("page.HTML")), MediaType::Html);
    assert_eq!(classify_media_type(Path::new("page.htm")), MediaType::Html);
    assert_eq!(classify_media_type(Path::new("book.epub")), MediaType::Epub);
    assert_eq!(classify_media_type(Path::new("notes.DOCX")), MediaType::Docx);
}

#[test]
fn extraction_contract_serializes_document_metadata_and_locations() {
    let document = ExtractedDocument {
        status: ExtractionStatus::TextReady,
        extractor_id: "fixture".into(),
        extractor_version: "v1".into(),
        title: Some("Guide".into()),
        links: vec![ExtractedLink {
            text: Some("Next".into()),
            target: "chapter-2.xhtml".into(),
            location: Some(SourceLocation::Epub {
                resource: "chapter-1.xhtml".into(),
                block: 2,
            }),
        }],
        blocks: vec![ExtractedBlock {
            heading: Some("Start".into()),
            text: "Read this.".into(),
            line_start: None,
            location: Some(SourceLocation::Epub {
                resource: "chapter-1.xhtml".into(),
                block: 1,
            }),
        }],
        warnings: Vec::new(),
    };

    let value = serde_json::to_value(document).unwrap();
    assert_eq!(value["title"], "Guide");
    assert_eq!(value["links"][0]["target"], "chapter-2.xhtml");
    assert_eq!(value["blocks"][0]["location"]["kind"], "epub");
    assert_eq!(value["blocks"][0]["location"]["resource"], "chapter-1.xhtml");
    assert_eq!(value["blocks"][0]["location"]["block"], 1);
}
#[test]
fn headings_inside_fences_are_not_sections_and_lines_remain_original() {
    let d=extract_bytes(MediaType::Markdown,b"---\ntitle: Setup\n---\n# Install\nRun cargo.\n\n```text\n# fake\n```\n## Verify\nRun tests.\n");
    assert_eq!(d.status, ExtractionStatus::TextReady);
    assert_eq!(d.blocks.len(), 2);
    assert_eq!(d.blocks[0].heading.as_deref(), Some("Install"));
    assert_eq!(d.blocks[0].line_start, Some(4));
    assert_eq!(d.blocks[1].line_start, Some(10));
    assert!(d.blocks[0].text.contains("# fake"));
}
#[test]
fn invalid_text_and_unsupported_formats_have_explicit_status() {
    assert_eq!(
        extract_bytes(MediaType::PlainText, &[255]).status,
        ExtractionStatus::MetadataOnly
    );
    assert_eq!(
        extract_bytes(MediaType::Pdf, b"%PDF").status,
        ExtractionStatus::MetadataOnly
    );
    let d = extract_bytes(MediaType::Json, b"{bad");
    assert!(!d.warnings.is_empty());
}
