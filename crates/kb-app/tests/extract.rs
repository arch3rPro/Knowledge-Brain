use kb_app::{classify_media_type, extract_bytes};
use kb_core::{
    ExtractedBlock, ExtractedDocument, ExtractedLink, ExtractionStatus, MediaType, SourceLocation,
};
use std::path::Path;
use std::{io::Write, io::Cursor};
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

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
fn html_extracts_visible_sections_title_and_links() {
    let document = extract_bytes(
        MediaType::Html,
        br#"<!doctype html><html><head><title>Rust &amp; Notes</title>
        <style>.secret { content: 'style noise'; }</style></head><body>
        <main><h1>Overview</h1><p>Visible &amp; searchable.</p>
        <script>script noise</script><h2>Next</h2>
        <p>Open <a href="chapter-2.html">chapter two</a>.</p></main>
        </body></html>"#,
    );

    assert_eq!(document.status, ExtractionStatus::TextReady);
    assert_eq!(document.extractor_id, "builtin-html");
    assert_eq!(document.title.as_deref(), Some("Rust & Notes"));
    assert_eq!(document.blocks.len(), 2);
    assert_eq!(document.blocks[0].heading.as_deref(), Some("Overview"));
    assert!(document.blocks[0].text.contains("Visible & searchable."));
    assert!(!document.blocks[0].text.contains("script noise"));
    assert!(!document.blocks[0].text.contains("style noise"));
    assert_eq!(document.blocks[1].heading.as_deref(), Some("Next"));
    assert_eq!(
        document.blocks[1].location,
        Some(SourceLocation::Html { block: 2 })
    );
    assert_eq!(
        document.links,
        vec![ExtractedLink {
            text: Some("chapter two".into()),
            target: "chapter-2.html".into(),
            location: None,
        }]
    );
}

#[test]
fn html_without_reliable_text_is_metadata_only() {
    let invalid = extract_bytes(MediaType::Html, &[0xff]);
    assert_eq!(invalid.status, ExtractionStatus::MetadataOnly);
    assert!(invalid.warnings[0].contains("UTF-8"));

    let empty = extract_bytes(MediaType::Html, b"<script>nothing visible</script>");
    assert_eq!(empty.status, ExtractionStatus::MetadataOnly);
    assert!(empty.blocks.is_empty());
}

#[test]
fn epub_uses_package_title_and_spine_order_with_locations() {
    let epub = zip_bytes(&[
        (
            "META-INF/container.xml",
            br#"<?xml version="1.0"?><container><rootfiles><rootfile full-path="OPS/package.opf"/></rootfiles></container>"#,
        ),
        (
            "OPS/package.opf",
            br#"<?xml version="1.0"?><package xmlns:dc="http://purl.org/dc/elements/1.1/">
            <metadata><dc:title>Portable Book</dc:title></metadata><manifest>
            <item id="first" href="text/z.xhtml" media-type="application/xhtml+xml"/>
            <item id="second" href="text/a.xhtml" media-type="application/xhtml+xml"/>
            </manifest><spine><itemref idref="first"/><itemref idref="second"/></spine></package>"#,
        ),
        (
            "OPS/text/a.xhtml",
            br#"<html><body><h1 id="end">Second</h1><p>Second chapter.</p></body></html>"#,
        ),
        (
            "OPS/text/z.xhtml",
            br#"<html><body><h1>First</h1><p>First chapter. <a href="a.xhtml#end">Continue</a></p></body></html>"#,
        ),
    ]);

    let document = extract_bytes(MediaType::Epub, &epub);

    assert_eq!(document.status, ExtractionStatus::TextReady);
    assert_eq!(document.extractor_id, "builtin-epub");
    assert_eq!(document.title.as_deref(), Some("Portable Book"));
    assert_eq!(document.blocks.len(), 2);
    assert!(document.blocks[0].text.contains("First chapter."));
    assert!(document.blocks[1].text.contains("Second chapter."));
    assert_eq!(
        document.blocks[0].location,
        Some(SourceLocation::Epub {
            resource: "OPS/text/z.xhtml".into(),
            block: 1,
        })
    );
    assert_eq!(document.links[0].target, "OPS/text/a.xhtml#end");
    assert_eq!(document.links[0].text.as_deref(), Some("Continue"));
}

#[test]
fn epub_rejects_invalid_or_excessively_expanded_archives() {
    let corrupt = extract_bytes(MediaType::Epub, b"not a zip archive");
    assert_eq!(corrupt.status, ExtractionStatus::MetadataOnly);
    assert!(corrupt.warnings[0].contains("EPUB"));

    let oversized = vec![b'x'; 16 * 1024 * 1024 + 1];
    let archive = zip_bytes(&[("META-INF/container.xml", oversized.as_slice())]);
    let document = extract_bytes(MediaType::Epub, &archive);
    assert_eq!(document.status, ExtractionStatus::MetadataOnly);
    assert!(document.warnings[0].contains("limit"));
}

fn zip_bytes(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    for (name, contents) in entries {
        writer.start_file(*name, options).unwrap();
        writer.write_all(contents).unwrap();
    }
    writer.finish().unwrap().into_inner()
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
