use crate::{
    extract_bytes,
    source_apply::integrity,
    source_io::{Budget, hash, io, list_files, safe_path},
    source_record,
};
use kb_core::{
    CURRENT_SCHEMA_VERSION, Catalog, CatalogEntry, EffectiveConfig, KbError, MediaType,
    PortableRelativePath, SearchGroup, SearchHit, SearchMatchMode, SearchMode, SearchRequest,
    SearchResponse, SearchScope,
};
use std::{cmp::Reverse, collections::BTreeSet, fs, path::Path};
pub(crate) struct Document {
    pub(crate) path: PortableRelativePath,
    pub(crate) content_path: PortableRelativePath,
    pub(crate) source_uri: Option<String>,
    pub(crate) title: String,
    pub(crate) media: MediaType,
    pub(crate) bytes: Vec<u8>,
    pub(crate) annotation: Option<String>,
}
pub(crate) fn documents(
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
    match r.match_mode {
        SearchMatchMode::Exact => {
            let groups = scopes
                .into_iter()
                .map(|scope| {
                    search_scope(
                        root,
                        scope,
                        c,
                        r.query.trim(),
                        r.limit,
                        SearchMatchMode::Exact,
                    )
                })
                .collect::<Result<_, _>>()?;
            return Ok(SearchResponse {
                schema_version: CURRENT_SCHEMA_VERSION,
                query: r.query.clone(),
                match_mode: r.match_mode,
                groups,
                warnings: Vec::new(),
            });
        }
        SearchMatchMode::Relevant => {}
    }
    let phrase = r.query.trim().to_lowercase();
    let (groups, warnings) = if c.search.mode.value == SearchMode::Bm25 {
        match scopes
            .iter()
            .copied()
            .map(|scope| crate::bm25::search(root, scope, c, &r.query, r.limit))
            .collect::<Result<Vec<_>, _>>()
        {
            Ok(groups) => (groups, Vec::new()),
            Err(error) if error.code == kb_core::ErrorCode::IndexStale && !r.strict_backend => (
                scopes
                    .into_iter()
                    .map(|scope| {
                        search_scope(root, scope, c, &phrase, r.limit, SearchMatchMode::Relevant)
                    })
                    .collect::<Result<_, _>>()?,
                vec![format!("{} Results use direct search.", error.message)],
            ),
            Err(error) => return Err(error),
        }
    } else {
        (
            scopes
                .into_iter()
                .map(|scope| {
                    search_scope(root, scope, c, &phrase, r.limit, SearchMatchMode::Relevant)
                })
                .collect::<Result<_, _>>()?,
            Vec::new(),
        )
    };
    Ok(SearchResponse {
        schema_version: CURRENT_SCHEMA_VERSION,
        query: r.query.clone(),
        match_mode: r.match_mode,
        groups,
        warnings,
    })
}

fn search_scope(
    root: &Path,
    scope: SearchScope,
    config: &EffectiveConfig,
    phrase: &str,
    limit: usize,
    match_mode: SearchMatchMode,
) -> Result<SearchGroup, KbError> {
    let terms = phrase.split_whitespace().collect::<BTreeSet<_>>();
    let mut hits = Vec::new();
    for document in documents(root, scope, config)? {
        let extracted = extract_bytes(document.media, &document.bytes);
        let document_title = extracted.title.unwrap_or_else(|| document.title.clone());
        let mut blocks = extracted
            .blocks
            .into_iter()
            .map(|block| (block, document.content_path.clone()))
            .collect::<Vec<_>>();
        if let Some(annotation) = &document.annotation {
            blocks.extend(
                extract_bytes(MediaType::Markdown, annotation.as_bytes())
                    .blocks
                    .into_iter()
                    .map(|block| (block, document.path.clone())),
            );
        }
        let before_hits = hits.len();
        for (block, content_path) in blocks {
            let (count, distinct, title_match) = match match_mode {
                SearchMatchMode::Relevant => {
                    let folded = block.text.to_lowercase();
                    (
                        folded.matches(phrase).count(),
                        terms.iter().filter(|term| folded.contains(**term)).count(),
                        block
                            .heading
                            .as_ref()
                            .unwrap_or(&document_title)
                            .to_lowercase()
                            .contains(phrase),
                    )
                }
                SearchMatchMode::Exact => (block.text.matches(phrase).count(), 0, false),
            };
            if count == 0 && distinct == 0 {
                continue;
            }
            let title = block
                .heading
                .clone()
                .unwrap_or_else(|| document_title.clone());
            let hit = SearchHit {
                path: document.path.clone(),
                content_path,
                source_uri: document.source_uri.clone(),
                title,
                heading: block.heading,
                line_start: block.line_start,
                location: block.location,
                snippet: snippet(&block.text, phrase, match_mode),
                match_count: count as u64,
                backend: Some(kb_core::SearchBackend::Direct),
                score_micros: None,
                explanation: None,
            };
            hits.push((Reverse(count), Reverse(distinct), Reverse(title_match), hit));
        }
        if match_mode == SearchMatchMode::Relevant
            && hits.len() == before_hits
            && document_title.to_lowercase().contains(phrase)
        {
            hits.push((
                Reverse(1),
                Reverse(terms.len()),
                Reverse(true),
                SearchHit {
                    path: document.path.clone(),
                    content_path: document.path,
                    source_uri: document.source_uri,
                    title: document_title.clone(),
                    heading: None,
                    line_start: None,
                    location: None,
                    snippet: document_title,
                    match_count: 1,
                    backend: Some(kb_core::SearchBackend::Direct),
                    score_micros: None,
                    explanation: None,
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
    Ok(SearchGroup {
        scope,
        results: hits
            .into_iter()
            .take(limit)
            .map(|(_, _, _, hit)| hit)
            .collect(),
    })
}
fn snippet(text: &str, phrase: &str, match_mode: SearchMatchMode) -> String {
    let line = text
        .lines()
        .find(|line| match match_mode {
            SearchMatchMode::Relevant => line.to_lowercase().contains(phrase),
            SearchMatchMode::Exact => line.contains(phrase),
        })
        .unwrap_or(text);
    line.chars().take(240).collect()
}

pub(crate) fn invalidate_caches(root: &Path) -> Vec<String> {
    let mut warnings = Vec::new();
    for relative in [".kb/cache/catalog.json", ".kb/cache/bm25.json"] {
        let outcome = safe_path(root, relative).and_then(|path| match fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(io("remove derived search cache", &path, error)),
        });
        if let Err(error) = outcome {
            warnings.push(format!(
                "Content saved; search cache invalidation failed: {}",
                error.message
            ));
        }
    }
    warnings
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
                title: e.title.unwrap_or(d.title),
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
    if c.search.mode.value == SearchMode::Bm25 {
        crate::bm25::update(root, c)?;
    }
    Ok(catalog)
}
