use super::archive::BoundedArchive;
use kb_core::{ExtractedBlock, ExtractedDocument, ExtractedLink, ExtractionStatus, SourceLocation};
use quick_xml::{Reader, events::Event};
use std::collections::BTreeMap;

#[derive(Default)]
struct Paragraph {
    number: u64,
    style: Option<String>,
    text: String,
    links: Vec<(String, String)>,
    active_link: Option<(String, String)>,
}

pub(super) fn extract(bytes: &[u8]) -> ExtractedDocument {
    match extract_inner(bytes) {
        Ok(document) => document,
        Err(warning) => ExtractedDocument {
            status: ExtractionStatus::MetadataOnly,
            extractor_id: "builtin-docx".into(),
            extractor_version: "v1".into(),
            title: None,
            links: Vec::new(),
            blocks: Vec::new(),
            warnings: vec![warning],
        },
    }
}

fn extract_inner(bytes: &[u8]) -> Result<ExtractedDocument, String> {
    let archive = BoundedArchive::open(bytes, "DOCX")?;
    let title = archive
        .required("docProps/core.xml", "DOCX")
        .ok()
        .and_then(|xml| parse_title(xml).ok().flatten());
    let styles = archive
        .required("word/styles.xml", "DOCX")
        .ok()
        .map_or_else(|| Ok(BTreeMap::new()), parse_styles)?;
    let relationships = archive
        .required("word/_rels/document.xml.rels", "DOCX")
        .ok()
        .map_or_else(|| Ok(BTreeMap::new()), parse_relationships)?;
    let (blocks, links) = parse_document(
        archive.required("word/document.xml", "DOCX")?,
        &styles,
        &relationships,
    )?;
    if blocks.is_empty() {
        return Err("DOCX does not contain readable paragraphs or tables.".to_owned());
    }
    Ok(ExtractedDocument {
        status: ExtractionStatus::TextReady,
        extractor_id: "builtin-docx".into(),
        extractor_version: "v1".into(),
        title,
        links,
        blocks,
        warnings: Vec::new(),
    })
}

fn parse_title(xml: &[u8]) -> Result<Option<String>, String> {
    let mut reader = Reader::from_reader(xml);
    let mut in_title = false;
    loop {
        match reader.read_event() {
            Ok(Event::Start(element)) if local(element.name().as_ref()) == b"title" => {
                in_title = true;
            }
            Ok(Event::Text(text)) if in_title => {
                let value = text
                    .xml_content()
                    .map_err(|error| format!("Cannot decode DOCX title: {error}"))?;
                let value = value.trim();
                return Ok((!value.is_empty()).then(|| value.to_owned()));
            }
            Ok(Event::End(element)) if local(element.name().as_ref()) == b"title" => {
                return Ok(None);
            }
            Ok(Event::Eof) => return Ok(None),
            Ok(_) => {}
            Err(error) => return Err(format!("Cannot parse DOCX core properties: {error}")),
        }
    }
}

fn parse_styles(xml: &[u8]) -> Result<BTreeMap<String, String>, String> {
    let mut reader = Reader::from_reader(xml);
    let mut styles = BTreeMap::new();
    let mut current = None;
    loop {
        match reader.read_event() {
            Ok(Event::Start(element)) if local(element.name().as_ref()) == b"style" => {
                current = attribute(&reader, &element, b"styleId")?;
            }
            Ok(Event::Start(element) | Event::Empty(element))
                if local(element.name().as_ref()) == b"name" =>
            {
                if let (Some(id), Some(name)) =
                    (current.as_ref(), attribute(&reader, &element, b"val")?)
                {
                    styles.insert(id.clone(), name);
                }
            }
            Ok(Event::End(element)) if local(element.name().as_ref()) == b"style" => {
                current = None;
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(error) => return Err(format!("Cannot parse DOCX styles: {error}")),
        }
    }
    Ok(styles)
}

fn parse_relationships(xml: &[u8]) -> Result<BTreeMap<String, String>, String> {
    let mut reader = Reader::from_reader(xml);
    let mut relationships = BTreeMap::new();
    loop {
        match reader.read_event() {
            Ok(Event::Start(element) | Event::Empty(element))
                if local(element.name().as_ref()) == b"Relationship" =>
            {
                let kind = attribute(&reader, &element, b"Type")?.unwrap_or_default();
                if kind.ends_with("/hyperlink") {
                    let id = attribute(&reader, &element, b"Id")?
                        .ok_or_else(|| "DOCX hyperlink relationship has no Id.".to_owned())?;
                    let target = attribute(&reader, &element, b"Target")?.ok_or_else(|| {
                        format!("DOCX hyperlink relationship {id} has no target.")
                    })?;
                    relationships.insert(id, target);
                }
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(error) => return Err(format!("Cannot parse DOCX relationships: {error}")),
        }
    }
    Ok(relationships)
}

// Keeping the XML event transitions together makes the parser state and nesting rules auditable.
#[allow(clippy::too_many_lines)]
fn parse_document(
    xml: &[u8],
    styles: &BTreeMap<String, String>,
    relationships: &BTreeMap<String, String>,
) -> Result<(Vec<ExtractedBlock>, Vec<ExtractedLink>), String> {
    let mut reader = Reader::from_reader(xml);
    let mut blocks = Vec::new();
    let mut links = Vec::new();
    let mut paragraph_count = 0_u64;
    let mut paragraph = None;
    let mut in_text = false;
    let mut table_count = 0_u64;
    let mut row_count = 0_u64;
    let mut in_table = false;
    let mut row_first_paragraph = None;
    let mut cell = String::new();
    let mut cells = Vec::new();
    loop {
        match reader.read_event() {
            Ok(Event::Start(element)) => match local(element.name().as_ref()) {
                b"tbl" => {
                    table_count += 1;
                    row_count = 0;
                    in_table = true;
                }
                b"tr" => {
                    row_count += 1;
                    row_first_paragraph = None;
                    cells.clear();
                }
                b"tc" => {
                    cell.clear();
                }
                b"p" => {
                    paragraph_count += 1;
                    row_first_paragraph.get_or_insert(paragraph_count);
                    paragraph = Some(Paragraph {
                        number: paragraph_count,
                        ..Paragraph::default()
                    });
                }
                b"pStyle" => {
                    if let Some(value) = attribute(&reader, &element, b"val")? {
                        if let Some(paragraph) = paragraph.as_mut() {
                            paragraph.style = Some(value);
                        }
                    }
                }
                b"hyperlink" => {
                    if let Some(id) = attribute(&reader, &element, b"id")? {
                        if let Some(paragraph) = paragraph.as_mut() {
                            paragraph.active_link = Some((id, String::new()));
                        }
                    }
                }
                b"t" => in_text = true,
                b"tab" => append_text(&mut paragraph, "\t"),
                b"br" => append_text(&mut paragraph, "\n"),
                _ => {}
            },
            Ok(Event::Empty(element)) => match local(element.name().as_ref()) {
                b"pStyle" => {
                    if let Some(value) = attribute(&reader, &element, b"val")? {
                        if let Some(paragraph) = paragraph.as_mut() {
                            paragraph.style = Some(value);
                        }
                    }
                }
                b"tab" => append_text(&mut paragraph, "\t"),
                b"br" => append_text(&mut paragraph, "\n"),
                _ => {}
            },
            Ok(Event::Text(text)) if in_text => {
                let value = text
                    .xml_content()
                    .map_err(|error| format!("Cannot decode DOCX text: {error}"))?;
                append_text(&mut paragraph, &value);
            }
            Ok(Event::End(element)) => match local(element.name().as_ref()) {
                b"t" => in_text = false,
                b"hyperlink" => {
                    if let Some(paragraph) = paragraph.as_mut() {
                        if let Some(link) = paragraph.active_link.take() {
                            paragraph.links.push(link);
                        }
                    }
                }
                b"p" => {
                    let finished = paragraph
                        .take()
                        .ok_or_else(|| "DOCX paragraph end has no matching start.".to_owned())?;
                    if in_table {
                        if !cell.is_empty() && !finished.text.is_empty() {
                            cell.push('\n');
                        }
                        cell.push_str(&finished.text);
                    } else {
                        finish_paragraph(finished, styles, relationships, &mut blocks, &mut links);
                    }
                }
                b"tc" => {
                    let value = cell.trim().to_owned();
                    if !value.is_empty() {
                        cells.push(value);
                    }
                }
                b"tr" => {
                    let text = cells.join(" | ");
                    if !text.is_empty() {
                        blocks.push(ExtractedBlock {
                            heading: None,
                            text,
                            line_start: None,
                            location: Some(SourceLocation::Docx {
                                paragraph: row_first_paragraph.unwrap_or(paragraph_count),
                                table: Some(table_count),
                                row: Some(row_count),
                            }),
                        });
                    }
                }
                b"tbl" => in_table = false,
                _ => {}
            },
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(error) => return Err(format!("Cannot parse DOCX document XML: {error}")),
        }
    }
    Ok((blocks, links))
}

fn append_text(paragraph: &mut Option<Paragraph>, value: &str) {
    if let Some(paragraph) = paragraph {
        paragraph.text.push_str(value);
        if let Some((_, label)) = paragraph.active_link.as_mut() {
            label.push_str(value);
        }
    }
}

fn finish_paragraph(
    paragraph: Paragraph,
    styles: &BTreeMap<String, String>,
    relationships: &BTreeMap<String, String>,
    blocks: &mut Vec<ExtractedBlock>,
    links: &mut Vec<ExtractedLink>,
) {
    let text = paragraph.text.trim().to_owned();
    if text.is_empty() {
        return;
    }
    let location = SourceLocation::Docx {
        paragraph: paragraph.number,
        table: None,
        row: None,
    };
    let is_heading = paragraph.style.as_ref().is_some_and(|style| {
        style.to_ascii_lowercase().starts_with("heading")
            || styles
                .get(style)
                .is_some_and(|name| name.to_ascii_lowercase().starts_with("heading"))
    });
    for (id, label) in paragraph.links {
        if let Some(target) = relationships.get(&id) {
            links.push(ExtractedLink {
                text: (!label.trim().is_empty()).then(|| label.trim().to_owned()),
                target: target.clone(),
                location: Some(location.clone()),
            });
        }
    }
    blocks.push(ExtractedBlock {
        heading: is_heading.then(|| text.clone()),
        text,
        line_start: None,
        location: Some(location),
    });
}

fn attribute(
    reader: &Reader<&[u8]>,
    element: &quick_xml::events::BytesStart<'_>,
    name: &[u8],
) -> Result<Option<String>, String> {
    for value in element.attributes() {
        let value = value.map_err(|error| format!("Invalid DOCX XML attribute: {error}"))?;
        if local(value.key.as_ref()) == name {
            return value
                .decode_and_unescape_value(reader.decoder())
                .map(|value| Some(value.into_owned()))
                .map_err(|error| format!("Cannot decode DOCX XML attribute: {error}"));
        }
    }
    Ok(None)
}

fn local(name: &[u8]) -> &[u8] {
    name.rsplit(|value| *value == b':').next().unwrap_or(name)
}
