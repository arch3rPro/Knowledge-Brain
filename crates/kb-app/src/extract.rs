use kb_core::{ExtractedBlock, ExtractedDocument, ExtractionStatus, Extractor, MediaType};
use std::path::Path;

mod archive;
mod docx;
mod epub;
mod html;
#[must_use]
pub fn classify_media_type(path: &Path) -> MediaType {
    match path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "md" | "markdown" => MediaType::Markdown,
        "txt" => MediaType::PlainText,
        "yaml" | "yml" => MediaType::Yaml,
        "json" => MediaType::Json,
        "csv" => MediaType::Csv,
        "html" | "htm" => MediaType::Html,
        "epub" => MediaType::Epub,
        "docx" => MediaType::Docx,
        "pdf" => MediaType::Pdf,
        _ => MediaType::Other,
    }
}
#[must_use]
pub fn extract_bytes(media: MediaType, bytes: &[u8]) -> ExtractedDocument {
    if BuiltinTextExtractor.supports(media) {
        BuiltinTextExtractor.extract(media, bytes)
    } else {
        match media {
            MediaType::Html => html::extract(bytes),
            MediaType::Epub => epub::extract(bytes),
            MediaType::Docx => docx::extract(bytes),
            _ => unavailable(media),
        }
    }
}
pub struct BuiltinTextExtractor;
impl Extractor for BuiltinTextExtractor {
    fn id(&self) -> &'static str {
        "builtin-text"
    }
    fn version(&self) -> &'static str {
        "v1"
    }
    fn supports(&self, media: MediaType) -> bool {
        matches!(
            media,
            MediaType::Markdown
                | MediaType::PlainText
                | MediaType::Yaml
                | MediaType::Json
                | MediaType::Csv
        )
    }
    fn extract(&self, media: MediaType, bytes: &[u8]) -> ExtractedDocument {
        extract_text(media, bytes)
    }
}
fn extract_text(media: MediaType, bytes: &[u8]) -> ExtractedDocument {
    let mut result = ExtractedDocument {
        status: ExtractionStatus::TextReady,
        extractor_id: "builtin-text".into(),
        extractor_version: "v1".into(),
        title: None,
        links: Vec::new(),
        blocks: Vec::new(),
        warnings: Vec::new(),
    };
    let Ok(text) = std::str::from_utf8(bytes) else {
        result.status = ExtractionStatus::MetadataOnly;
        result.warnings.push("Expected UTF-8 text.".into());
        return result;
    };
    if text.contains('\0') {
        result.status = ExtractionStatus::MetadataOnly;
        result.warnings.push("Text contains NUL bytes.".into());
        return result;
    }
    match media {
        MediaType::Json => {
            if let Err(e) = serde_json::from_str::<serde_json::Value>(text) {
                result.warnings.push(e.to_string());
            }
        }
        MediaType::Yaml => {
            if let Err(e) = serde_yaml_ng::from_str::<serde_yaml_ng::Value>(text) {
                result.warnings.push(e.to_string());
            }
        }
        _ => {}
    }
    if media == MediaType::Markdown {
        result.blocks = markdown_blocks(text);
    } else {
        result.blocks.push(ExtractedBlock {
            heading: None,
            text: text.to_owned(),
            line_start: Some(1),
            location: None,
        });
    }
    result
}
pub(super) fn markdown_blocks(text: &str) -> Vec<ExtractedBlock> {
    let lines = text.lines().collect::<Vec<_>>();
    let start = if lines.first() == Some(&"---") {
        lines
            .iter()
            .skip(1)
            .position(|s| *s == "---")
            .map_or(0, |i| i + 2)
    } else {
        0
    };
    let mut blocks = Vec::new();
    let mut current = ExtractedBlock {
        heading: None,
        text: String::new(),
        line_start: Some((start + 1) as u64),
        location: None,
    };
    let mut fence: Option<(char, usize)> = None;
    for (i, line) in lines.iter().enumerate().skip(start) {
        let trim = line.trim_start();
        let marker = trim.chars().next().unwrap_or(' ');
        let count = trim.chars().take_while(|c| *c == marker).count();
        if matches!(marker, '`' | '~') && count >= 3 {
            if let Some((open, n)) = fence {
                if marker == open && count >= n && trim[count..].trim().is_empty() {
                    fence = None;
                }
            } else {
                fence = Some((marker, count));
            }
        }
        let hashes = trim.chars().take_while(|c| *c == '#').count();
        if fence.is_none() && (1..=6).contains(&hashes) && trim[hashes..].starts_with(' ') {
            if !current.text.trim().is_empty() {
                blocks.push(current);
            }
            current = ExtractedBlock {
                heading: Some(trim[hashes..].trim().trim_end_matches('#').trim().into()),
                text: String::new(),
                line_start: Some((i + 1) as u64),
                location: None,
            };
        }
        current.text.push_str(line);
        current.text.push('\n');
    }
    if !current.text.trim().is_empty() {
        blocks.push(current);
    }
    blocks
}

fn unavailable(media: MediaType) -> ExtractedDocument {
    let (status, warning) = match media {
        MediaType::Pdf => (
            ExtractionStatus::MetadataOnly,
            Some("PDF text extraction is a future extension."),
        ),
        MediaType::Html | MediaType::Epub | MediaType::Docx => (
            ExtractionStatus::MetadataOnly,
            Some("The built-in document extractor is unavailable."),
        ),
        MediaType::Other => (ExtractionStatus::Unsupported, None),
        MediaType::Markdown
        | MediaType::PlainText
        | MediaType::Yaml
        | MediaType::Json
        | MediaType::Csv => unreachable!("text formats are routed to BuiltinTextExtractor"),
    };
    ExtractedDocument {
        status,
        extractor_id: "none".into(),
        extractor_version: "v1".into(),
        title: None,
        links: Vec::new(),
        blocks: Vec::new(),
        warnings: warning.into_iter().map(str::to_owned).collect(),
    }
}
