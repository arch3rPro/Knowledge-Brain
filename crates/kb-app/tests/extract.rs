use kb_app::extract_bytes;
use kb_core::{ExtractionStatus, MediaType};
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
        ExtractionStatus::Unsupported
    );
    let d = extract_bytes(MediaType::Json, b"{bad");
    assert!(!d.warnings.is_empty());
}
