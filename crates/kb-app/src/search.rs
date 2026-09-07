use crate::{
    extract_bytes,
    source_apply::integrity,
    source_io::{Budget, hash, io, list_files, safe_path},
    source_record,
};
use kb_core::{
    CURRENT_SCHEMA_VERSION, Catalog, CatalogEntry, EffectiveConfig, KbError, MediaType,
    PortableRelativePath, SearchGroup, SearchHit, SearchMode, SearchRequest, SearchResponse,
    SearchScope,
};
use std::{cmp::Reverse, collections::BTreeSet, fs, path::Path};
struct Document {
    path: PortableRelativePath,
    content_path: PortableRelativePath,
    source_uri: Option<String>,
    title: String,
    media: MediaType,
    bytes: Vec<u8>,
    annotation: Option<String>,
}
fn documents(
    root: &Path,
    scope: SearchScope,
    c: &EffectiveConfig,
) -> Result<Vec<Document>, KbError> {
    let mut out = Vec::new();
    let mut b = Budget::new(c);
    if scope == SearchScope::Wiki {
        let mut paths = Vec::new();
        for dir in ["Wiki/research", "Wiki/articles"] {
            paths.extend(list_files(root, dir, &mut b)?);
        }
        if safe_path(root, "Wiki/index.md")?.is_file() {
            paths.push(PortableRelativePath::parse("Wiki/index.md")?);
        }
        for path in paths {
            if !path.as_str().to_ascii_lowercase().ends_with(".md") {
                continue;
            }
            let bytes = b.read(&safe_path(root, path.as_str())?)?;
            let title = path
                .as_str()
                .rsplit('/')
                .next()
                .unwrap_or("document")
                .into();
            out.push(Document {
                content_path: path.clone(),
                path,
                source_uri: None,
                title,
                media: MediaType::Markdown,
                bytes,
                annotation: None,
            });
        }
    } else {
        for (_, stored) in source_record::inventory(root, c, &mut b)? {
            let r = stored.record;
            let object = r.source.object_path();
            let bytes = b.read(&safe_path(root, &object)?)?;
            if hash(&bytes) != r.source.sha256() {
                return Err(integrity(&object));
            }
            out.push(Document {
                path: stored.path,
                content_path: PortableRelativePath::parse(&object)?,
                source_uri: Some(r.source.exact_uri()),
                title: r.title,
                media: r.media_type,
                bytes,
                annotation: Some(stored.markdown),
            });
        }
    }
    Ok(out)
}
/// Search actual Wiki Markdown and captured source objects, independent of caches.
/// # Errors
/// Reports invalid queries, path/size limits, IO and source integrity failures.
pub fn query(
    root: &Path,
    r: &SearchRequest,
    c: &EffectiveConfig,
) -> Result<SearchResponse, KbError> {
    r.validate()?;
    let scopes = if r.scope == SearchScope::All {
        vec![SearchScope::Wiki, SearchScope::Sources]
    } else {
        vec![r.scope]
    };
    let phrase = r.query.trim().to_lowercase();
    let terms = phrase.split_whitespace().collect::<BTreeSet<_>>();
    let mut groups = Vec::new();
    for scope in scopes {
        let mut hits = Vec::new();
        for d in documents(root, scope, c)? {
            let mut blocks = extract_bytes(d.media, &d.bytes)
                .blocks
                .into_iter()
                .map(|b| (b, d.content_path.clone()))
                .collect::<Vec<_>>();
            if let Some(annotation) = &d.annotation {
                blocks.extend(
                    extract_bytes(MediaType::Markdown, annotation.as_bytes())
                        .blocks
                        .into_iter()
                        .map(|b| (b, d.path.clone())),
                );
            }
            let before_hits = hits.len();
            for (block, content_path) in blocks {
                let folded = block.text.to_lowercase();
                let count = folded.matches(&phrase).count();
                let distinct = terms.iter().filter(|t| folded.contains(**t)).count();
                if count == 0 && distinct == 0 {
                    continue;
                }
                let title = block.heading.clone().unwrap_or_else(|| d.title.clone());
                let title_match = title.to_lowercase().contains(&phrase);
                let hit = SearchHit {
                    path: d.path.clone(),
                    content_path,
                    source_uri: d.source_uri.clone(),
                    title,
                    heading: block.heading,
                    line_start: block.line_start,
                    snippet: snippet(&block.text, &phrase),
                    match_count: count as u64,
                };
                hits.push((Reverse(count), Reverse(distinct), Reverse(title_match), hit));
            }
            if hits.len() == before_hits && d.title.to_lowercase().contains(&phrase) {
                hits.push((
                    Reverse(1),
                    Reverse(terms.len()),
                    Reverse(true),
                    SearchHit {
                        path: d.path.clone(),
                        content_path: d.path,
                        source_uri: d.source_uri,
                        title: d.title.clone(),
                        heading: None,
                        line_start: None,
                        snippet: d.title,
                        match_count: 1,
                    },
                ));
            }
        }
        hits.sort_by(|a, b| {
            (&a.0, &a.1, &a.2, &a.3.path, &a.3.line_start).cmp(&(
                &b.0,
                &b.1,
                &b.2,
                &b.3.path,
                &b.3.line_start,
            ))
        });
        groups.push(SearchGroup {
            scope,
            results: hits
                .into_iter()
                .take(r.limit)
                .map(|(_, _, _, hit)| hit)
                .collect(),
        });
    }
    let warnings = if c.search.mode.value == SearchMode::Bm25 {
        vec!["BM25 is unavailable; results use direct search.".into()]
    } else {
        Vec::new()
    };
    Ok(SearchResponse {
        schema_version: CURRENT_SCHEMA_VERSION,
        query: r.query.clone(),
        groups,
        warnings,
    })
}
fn snippet(text: &str, phrase: &str) -> String {
    let line = text
        .lines()
        .find(|l| l.to_lowercase().contains(phrase))
        .unwrap_or(text);
    line.chars().take(240).collect()
}
/// Rebuild navigation metadata from authoritative content.
/// # Errors
/// Returns read, integrity or cache-write errors.
pub fn rebuild_catalog(root: &Path, c: &EffectiveConfig) -> Result<Catalog, KbError> {
    let mut entries = Vec::new();
    for scope in [SearchScope::Wiki, SearchScope::Sources] {
        for d in documents(root, scope, c)? {
            let e = extract_bytes(d.media, &d.bytes);
            entries.push(CatalogEntry {
                scope,
                path: d.path,
                sha256: hash(
                    d.annotation
                        .as_ref()
                        .map_or(d.bytes.as_slice(), |s| s.as_bytes()),
                ),
                title: d.title,
                headings: e.blocks.into_iter().filter_map(|b| b.heading).collect(),
            });
        }
    }
    entries.sort_by(|a, b| (&a.scope, &a.path).cmp(&(&b.scope, &b.path)));
    let catalog = Catalog {
        schema_version: CURRENT_SCHEMA_VERSION,
        indexer_version: "catalog-v1".into(),
        entries,
    };
    let p = safe_path(root, ".kb/cache/catalog.json")?;
    let parent = safe_path(root, ".kb/cache")?;
    fs::create_dir_all(&parent).map_err(|e| io("create cache directory", &parent, e))?;
    crate::operation::write_json(&p, &catalog)?;
    Ok(catalog)
}
