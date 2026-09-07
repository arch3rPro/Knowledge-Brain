use super::{archive::BoundedArchive, archive::resolve, html};
use kb_core::{ExtractedDocument, ExtractionStatus, SourceLocation};
use quick_xml::{Reader, events::Event};
use std::collections::BTreeMap;

struct Package {
    title: Option<String>,
    manifest: BTreeMap<String, String>,
    spine: Vec<String>,
}

pub(super) fn extract(bytes: &[u8]) -> ExtractedDocument {
    match extract_inner(bytes) {
        Ok(document) => document,
        Err(warning) => ExtractedDocument {
            status: ExtractionStatus::MetadataOnly,
            extractor_id: "builtin-epub".into(),
            extractor_version: "v1".into(),
            title: None,
            links: Vec::new(),
            blocks: Vec::new(),
            warnings: vec![warning],
        },
    }
}

fn extract_inner(bytes: &[u8]) -> Result<ExtractedDocument, String> {
    let archive = BoundedArchive::open(bytes, "EPUB")?;
    let container = archive.required("META-INF/container.xml", "EPUB")?;
    let package_path = rootfile(container)?;
    let package = parse_package(archive.required(&package_path, "EPUB")?)?;
    let mut blocks = Vec::new();
    let mut links = Vec::new();
    for id in &package.spine {
        let href = package
            .manifest
            .get(id)
            .ok_or_else(|| format!("EPUB spine references unknown manifest item: {id}"))?;
        let resource = resolve(&package_path, href)?;
        let content = html::parse(archive.required(&resource, "EPUB")?)?;
        for (index, mut block) in content.sections.into_iter().enumerate() {
            block.line_start = None;
            block.location = Some(SourceLocation::Epub {
                resource: resource.clone(),
                block: index as u64 + 1,
            });
            blocks.push(block);
        }
        for mut link in content.links {
            link.target = resolve(&resource, &link.target)?;
            links.push(link);
        }
    }
    if blocks.is_empty() {
        return Err("EPUB does not contain readable spine text.".to_owned());
    }
    Ok(ExtractedDocument {
        status: ExtractionStatus::TextReady,
        extractor_id: "builtin-epub".into(),
        extractor_version: "v1".into(),
        title: package.title,
        links,
        blocks,
        warnings: Vec::new(),
    })
}

fn rootfile(xml: &[u8]) -> Result<String, String> {
    let mut reader = Reader::from_reader(xml);
    loop {
        match reader.read_event() {
            Ok(Event::Start(element) | Event::Empty(element))
                if local(element.name().as_ref()) == b"rootfile" =>
            {
                return attribute(&reader, &element, b"full-path")?
                    .ok_or_else(|| "EPUB rootfile has no full-path.".to_owned());
            }
            Ok(Event::Eof) => return Err("EPUB container has no rootfile.".to_owned()),
            Ok(_) => {}
            Err(error) => return Err(format!("Cannot parse EPUB container XML: {error}")),
        }
    }
}

fn parse_package(xml: &[u8]) -> Result<Package, String> {
    let mut reader = Reader::from_reader(xml);
    reader.config_mut().trim_text(true);
    let mut title = None;
    let mut in_title = false;
    let mut manifest = BTreeMap::new();
    let mut spine = Vec::new();
    loop {
        match reader.read_event() {
            Ok(Event::Start(element)) => match local(element.name().as_ref()) {
                b"title" => in_title = true,
                b"item" => add_manifest_item(&reader, &element, &mut manifest)?,
                b"itemref" => add_spine_item(&reader, &element, &mut spine)?,
                _ => {}
            },
            Ok(Event::Empty(element)) => match local(element.name().as_ref()) {
                b"item" => add_manifest_item(&reader, &element, &mut manifest)?,
                b"itemref" => add_spine_item(&reader, &element, &mut spine)?,
                _ => {}
            },
            Ok(Event::Text(text)) if in_title => {
                let value = text
                    .xml_content()
                    .map_err(|error| format!("Cannot decode EPUB title: {error}"))?;
                let value = value.trim();
                if !value.is_empty() && title.is_none() {
                    title = Some(value.to_owned());
                }
            }
            Ok(Event::End(element)) if local(element.name().as_ref()) == b"title" => {
                in_title = false;
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(error) => return Err(format!("Cannot parse EPUB package XML: {error}")),
        }
    }
    if spine.is_empty() {
        return Err("EPUB package has no spine items.".to_owned());
    }
    Ok(Package {
        title,
        manifest,
        spine,
    })
}

fn add_manifest_item(
    reader: &Reader<&[u8]>,
    element: &quick_xml::events::BytesStart<'_>,
    manifest: &mut BTreeMap<String, String>,
) -> Result<(), String> {
    let id = attribute(reader, element, b"id")?
        .ok_or_else(|| "EPUB manifest item has no id.".to_owned())?;
    let href = attribute(reader, element, b"href")?
        .ok_or_else(|| format!("EPUB manifest item {id} has no href."))?;
    if manifest.insert(id.clone(), href).is_some() {
        return Err(format!("EPUB manifest contains duplicate id: {id}"));
    }
    Ok(())
}

fn add_spine_item(
    reader: &Reader<&[u8]>,
    element: &quick_xml::events::BytesStart<'_>,
    spine: &mut Vec<String>,
) -> Result<(), String> {
    let id = attribute(reader, element, b"idref")?
        .ok_or_else(|| "EPUB spine item has no idref.".to_owned())?;
    spine.push(id);
    Ok(())
}

fn attribute(
    reader: &Reader<&[u8]>,
    element: &quick_xml::events::BytesStart<'_>,
    name: &[u8],
) -> Result<Option<String>, String> {
    for value in element.attributes() {
        let value = value.map_err(|error| format!("Invalid EPUB XML attribute: {error}"))?;
        if local(value.key.as_ref()) == name {
            return value
                .decode_and_unescape_value(reader.decoder())
                .map(|value| Some(value.into_owned()))
                .map_err(|error| format!("Cannot decode EPUB XML attribute: {error}"));
        }
    }
    Ok(None)
}

fn local(name: &[u8]) -> &[u8] {
    name.rsplit(|value| *value == b':').next().unwrap_or(name)
}
