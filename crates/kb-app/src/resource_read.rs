use crate::{
    extract_bytes,
    source_apply::integrity,
    source_io::{Budget, hash, safe_path},
    source_record,
};
use kb_core::{
    EffectiveConfig, ErrorCode, ExtractionStatus, KbError, KnowledgeResourceUri, MediaType,
    PortableRelativePath, ResourceKind, ResourceLink, ResourceReadRequest, ResourceReadResponse,
};
use std::path::{Component, Path};

/// Read one typed knowledge resource from the fixed Vault.
///
/// # Errors
/// Returns a stable resource error for invalid, stale, missing, unreadable, or
/// out-of-scope resources and preserves IO/integrity errors from storage.
pub fn read_resource(
    root: &Path,
    config: &EffectiveConfig,
    request: &ResourceReadRequest,
) -> Result<ResourceReadResponse, KbError> {
    let uri = request.validate()?;
    let canonical_uri = uri.to_string();
    let loaded = match uri {
        KnowledgeResourceUri::Vault { vault_id, path } => {
            let identity = crate::vault::read_vault_identity(root)?;
            if identity.vault_id != vault_id {
                return Err(resource_error(
                    ErrorCode::ResourceOutOfScope,
                    &canonical_uri,
                    "Resource belongs to a different Vault.",
                    "Use a resource_uri returned by this Knowledge-Brain server.",
                ));
            }
            read_vault_resource(root, config, vault_id, &path, &canonical_uri)?
        }
        KnowledgeResourceUri::Source(version) => {
            read_source_resource(root, config, &version, &canonical_uri)?
        }
    };
    paginate(loaded, request)
}

struct LoadedResource {
    uri: String,
    kind: ResourceKind,
    content_type: String,
    content: Option<String>,
    sha256: String,
    links: Vec<ResourceLink>,
    warnings: Vec<String>,
}

fn read_vault_resource(
    root: &Path,
    config: &EffectiveConfig,
    vault_id: uuid::Uuid,
    path: &PortableRelativePath,
    uri: &str,
) -> Result<LoadedResource, KbError> {
    let file = safe_path(root, path.as_str())?;
    if !file.is_file() {
        return Err(resource_error(
            ErrorCode::ResourceNotFound,
            uri,
            "Knowledge resource does not exist.",
            "Query the Vault again and use a current resource_uri.",
        ));
    }
    let mut budget = Budget::new(config);
    let bytes = budget.read(&file)?;
    let text = String::from_utf8(bytes.clone()).map_err(|_| {
        resource_error(
            ErrorCode::ResourceNotReadable,
            uri,
            "Markdown resource is not valid UTF-8.",
            "Repair the Markdown file and retry.",
        )
    })?;
    let links = markdown_links(root, vault_id, path, &text)?;
    Ok(LoadedResource {
        uri: uri.to_owned(),
        kind: if path.as_str() == "KB.md" {
            ResourceKind::Rules
        } else {
            ResourceKind::Wiki
        },
        content_type: "text/markdown".into(),
        content: Some(text),
        sha256: hash(&bytes),
        links,
        warnings: Vec::new(),
    })
}

fn read_source_resource(
    root: &Path,
    config: &EffectiveConfig,
    version: &kb_core::SourceVersion,
    uri: &str,
) -> Result<LoadedResource, KbError> {
    let mut budget = Budget::new(config);
    let inventory = source_record::inventory(root, config, &mut budget)?;
    let stored = inventory.get(&version.source).ok_or_else(|| {
        resource_error(
            ErrorCode::ResourceNotFound,
            uri,
            "Saved source record does not exist.",
            "Query saved sources again and use a current resource_uri.",
        )
    })?;
    let known = stored.record.source == *version || stored.record.versions.contains(version);
    if !known {
        return Err(resource_error(
            ErrorCode::ResourceNotFound,
            uri,
            "Exact saved source version does not exist.",
            "Query saved sources again and use an exact returned version.",
        ));
    }
    let object = version.object_path();
    let object_path = safe_path(root, &object)?;
    if !object_path.is_file() {
        return Err(resource_error(
            ErrorCode::ResourceNotFound,
            uri,
            "Saved source object does not exist.",
            "Run source verification and restore the missing saved object.",
        ));
    }
    let bytes = budget.read(&object_path)?;
    if hash(&bytes) != version.sha256() {
        return Err(integrity(&object));
    }
    let extracted = extract_bytes(stored.record.media_type, &bytes);
    let content = if matches!(
        stored.record.media_type,
        MediaType::Markdown
            | MediaType::PlainText
            | MediaType::Yaml
            | MediaType::Json
            | MediaType::Csv
    ) && extracted.status == ExtractionStatus::TextReady
    {
        std::str::from_utf8(&bytes).ok().map(str::to_owned)
    } else if extracted.status == ExtractionStatus::TextReady {
        Some(
            extracted
                .blocks
                .iter()
                .map(|block| block.text.trim_end())
                .collect::<Vec<_>>()
                .join("\n\n"),
        )
    } else {
        None
    };
    let links = extracted
        .links
        .into_iter()
        .map(|link| ResourceLink {
            label: link.text.unwrap_or_else(|| link.target.clone()),
            external: link.target.starts_with("https://") || link.target.starts_with("http://"),
            target_uri: link.target,
            exists: None,
        })
        .collect();
    Ok(LoadedResource {
        uri: uri.to_owned(),
        kind: ResourceKind::Source,
        content_type: media_content_type(stored.record.media_type).into(),
        content,
        sha256: version.sha256().to_owned(),
        links,
        warnings: extracted.warnings,
    })
}

fn paginate(
    loaded: LoadedResource,
    request: &ResourceReadRequest,
) -> Result<ResourceReadResponse, KbError> {
    let text_available = loaded.content.is_some();
    let Some(content) = loaded.content else {
        return Ok(ResourceReadResponse {
            resource_uri: loaded.uri,
            kind: loaded.kind,
            content_type: loaded.content_type,
            content: None,
            sha256: loaded.sha256,
            text_available,
            complete: true,
            next_cursor: None,
            links: loaded.links,
            warnings: loaded.warnings,
        });
    };
    let uri_hash = hash(loaded.uri.as_bytes());
    let offset = match request.cursor.as_deref() {
        None => 0,
        Some(cursor) => parse_cursor(cursor, &uri_hash, &loaded.sha256, &loaded.uri)?,
    };
    let chars = content.chars().collect::<Vec<_>>();
    if offset > chars.len() {
        return Err(stale_cursor(&loaded.uri));
    }
    let end = offset.saturating_add(request.max_chars).min(chars.len());
    let page = chars[offset..end].iter().collect::<String>();
    let complete = end == chars.len();
    let next_cursor = (!complete).then(|| format!("v1:{uri_hash}:{}:{end}", loaded.sha256));
    Ok(ResourceReadResponse {
        resource_uri: loaded.uri,
        kind: loaded.kind,
        content_type: loaded.content_type,
        content: Some(page),
        sha256: loaded.sha256,
        text_available,
        complete,
        next_cursor,
        links: loaded.links,
        warnings: loaded.warnings,
    })
}

fn parse_cursor(cursor: &str, uri_hash: &str, sha: &str, uri: &str) -> Result<usize, KbError> {
    let mut parts = cursor.split(':');
    let valid =
        parts.next() == Some("v1") && parts.next() == Some(uri_hash) && parts.next() == Some(sha);
    let offset = parts.next().and_then(|value| value.parse::<usize>().ok());
    if !valid || parts.next().is_some() || offset.is_none() {
        return Err(stale_cursor(uri));
    }
    Ok(offset.expect("offset checked above"))
}

fn stale_cursor(uri: &str) -> KbError {
    resource_error(
        ErrorCode::ResourceCursorStale,
        uri,
        "Resource changed or the cursor does not belong to this resource version.",
        "Restart reading this resource without a cursor.",
    )
}

fn markdown_links(
    root: &Path,
    vault_id: uuid::Uuid,
    source: &PortableRelativePath,
    text: &str,
) -> Result<Vec<ResourceLink>, KbError> {
    let mut links = Vec::new();
    let mut remaining = text;
    while let Some(label_end) = remaining.find("](") {
        let before = &remaining[..label_end];
        let Some(label_start) = before.rfind('[') else {
            remaining = &remaining[label_end + 2..];
            continue;
        };
        let destination_start = label_end + 2;
        let Some(destination_end) = remaining[destination_start..].find(')') else {
            break;
        };
        let label = &remaining[label_start + 1..label_end];
        let destination = &remaining[destination_start..destination_start + destination_end];
        if destination.starts_with("https://") || destination.starts_with("http://") {
            links.push(ResourceLink {
                label: label.to_owned(),
                target_uri: destination.to_owned(),
                exists: None,
                external: true,
            });
        } else if let Some(target) = resolve_wiki_link(source, destination) {
            let path = PortableRelativePath::parse(&target)?;
            let exists = safe_path(root, path.as_str())?.is_file();
            links.push(ResourceLink {
                label: label.to_owned(),
                target_uri: format!("kb-vault://{vault_id}/{}", path.as_str()),
                exists: Some(exists),
                external: false,
            });
        }
        remaining = &remaining[destination_start + destination_end + 1..];
    }
    Ok(links)
}

fn resolve_wiki_link(source: &PortableRelativePath, destination: &str) -> Option<String> {
    let target = destination.split('#').next().unwrap_or_default();
    if target.is_empty() || target.starts_with('/') || target.contains(':') {
        return None;
    }
    let base = Path::new(source.as_str()).parent()?.join(target);
    let mut parts = Vec::new();
    for component in base.components() {
        match component {
            Component::Normal(value) => parts.push(value.to_str()?.to_owned()),
            Component::ParentDir => {
                parts.pop()?;
            }
            Component::CurDir => {}
            _ => return None,
        }
    }
    let normalized = parts.join("/");
    (normalized.starts_with("Wiki/") && normalized.to_ascii_lowercase().ends_with(".md"))
        .then_some(normalized)
}

const fn media_content_type(media: MediaType) -> &'static str {
    match media {
        MediaType::Markdown => "text/markdown",
        MediaType::PlainText => "text/plain",
        MediaType::Yaml => "application/yaml",
        MediaType::Json => "application/json",
        MediaType::Csv => "text/csv",
        MediaType::Html => "text/html",
        MediaType::Epub => "application/epub+zip",
        MediaType::Docx => {
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
        }
        MediaType::Pdf => "application/pdf",
        MediaType::Other => "application/octet-stream",
    }
}

fn resource_error(code: ErrorCode, uri: &str, message: &str, next_action: &str) -> KbError {
    KbError::new(code, message, false, next_action)
        .with_details(serde_json::json!({ "resource_uri": uri }))
}
