use kb_core::{
    ExtractedDocument, ExtractedLink, ExtractionStatus, SourceLocation,
};
use scraper::{Html, Selector};

const RENDER_WIDTH: usize = 120;

pub(super) struct HtmlContent {
    pub title: Option<String>,
    pub links: Vec<ExtractedLink>,
    pub sections: Vec<kb_core::ExtractedBlock>,
}

pub(super) fn extract(bytes: &[u8]) -> ExtractedDocument {
    match parse(bytes) {
        Ok(mut content) if !content.sections.is_empty() => {
            for (index, block) in content.sections.iter_mut().enumerate() {
                block.line_start = None;
                block.location = Some(SourceLocation::Html {
                    block: index as u64 + 1,
                });
            }
            ExtractedDocument {
                status: ExtractionStatus::TextReady,
                extractor_id: "builtin-html".into(),
                extractor_version: "v1".into(),
                title: content.title,
                links: content.links,
                blocks: content.sections,
                warnings: Vec::new(),
            }
        }
        Ok(content) => metadata_only(
            content.title,
            content.links,
            "HTML does not contain visible text.",
        ),
        Err(warning) => metadata_only(None, Vec::new(), &warning),
    }
}

pub(super) fn parse(bytes: &[u8]) -> Result<HtmlContent, String> {
    let source = std::str::from_utf8(bytes)
        .map_err(|_| "Expected UTF-8 HTML input.".to_owned())?;
    let dom = Html::parse_document(source);
    let title_selector = Selector::parse("title").expect("static selector is valid");
    let link_selector = Selector::parse("a[href]").expect("static selector is valid");
    let title = dom
        .select(&title_selector)
        .next()
        .map(|element| normalized_text(element.text()))
        .filter(|value| !value.is_empty());
    let links = dom
        .select(&link_selector)
        .filter_map(|element| {
            let target = element.value().attr("href")?.trim();
            if target.is_empty() {
                return None;
            }
            let text = normalized_text(element.text());
            Some(ExtractedLink {
                text: (!text.is_empty()).then_some(text),
                target: target.to_owned(),
                location: None,
            })
        })
        .collect();
    let rendered = html2text::from_read(source.as_bytes(), RENDER_WIDTH)
        .map_err(|error| format!("Cannot render HTML: {error}"))?;
    let sections = super::markdown_blocks(rendered.trim());
    Ok(HtmlContent {
        title,
        links,
        sections,
    })
}

fn normalized_text<'a>(parts: impl Iterator<Item = &'a str>) -> String {
    parts
        .flat_map(str::split_whitespace)
        .collect::<Vec<_>>()
        .join(" ")
}

fn metadata_only(
    title: Option<String>,
    links: Vec<ExtractedLink>,
    warning: &str,
) -> ExtractedDocument {
    ExtractedDocument {
        status: ExtractionStatus::MetadataOnly,
        extractor_id: "builtin-html".into(),
        extractor_version: "v1".into(),
        title,
        links,
        blocks: Vec::new(),
        warnings: vec![warning.to_owned()],
    }
}
