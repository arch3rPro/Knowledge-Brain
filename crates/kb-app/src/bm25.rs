use crate::{
    extract_bytes,
    search::{Document, documents},
    source_io::{hash, io, safe_path},
};
use kb_core::{
    CURRENT_SCHEMA_VERSION, EffectiveConfig, ErrorCode, KbError, PortableRelativePath,
    SchemaVersion, SearchBackend, SearchExplanation, SearchField, SearchFieldContribution,
    SearchGroup, SearchHit, SearchScope, SourceLocation, parse_okf,
};
use serde::{Deserialize, Serialize};
use serde_yaml_ng::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

const INDEXER_VERSION: &str = "bm25f-v1";
const K1: f64 = 1.2;
const B: f64 = 0.75;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Index {
    schema_version: SchemaVersion,
    indexer_version: String,
    generation: u64,
    parameters: Parameters,
    documents: Vec<IndexedDocument>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Parameters {
    k1_milli: u32,
    b_milli: u32,
    field_weights_milli: BTreeMap<SearchField, u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct IndexedDocument {
    scope: SearchScope,
    path: PortableRelativePath,
    content_path: PortableRelativePath,
    source_uri: Option<String>,
    fingerprint: String,
    generation: u64,
    title: String,
    chunks: Vec<Chunk>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Chunk {
    content_path: PortableRelativePath,
    heading: Option<String>,
    line_start: Option<u64>,
    location: Option<SourceLocation>,
    text: String,
    fields: BTreeMap<SearchField, FieldTerms>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FieldTerms {
    length: u32,
    frequencies: BTreeMap<String, u32>,
}

struct ChunkSeed {
    heading: Option<String>,
    line_start: Option<u64>,
    location: Option<SourceLocation>,
    text: String,
    content_path: PortableRelativePath,
}

pub(crate) fn update(root: &Path, config: &EffectiveConfig) -> Result<(), KbError> {
    let path = safe_path(root, ".kb/cache/bm25.json")?;
    let old = read_index(&path).ok();
    let generation = old.as_ref().map_or(1, |index| index.generation + 1);
    let mut previous = old
        .map(|index| {
            index
                .documents
                .into_iter()
                .map(|document| ((document.scope, document.path.clone()), document))
                .collect::<BTreeMap<_, _>>()
        })
        .unwrap_or_default();
    let mut indexed = Vec::new();
    for scope in [SearchScope::Wiki, SearchScope::Sources] {
        for document in documents(root, scope, config)? {
            let fingerprint = fingerprint(&document);
            let key = (scope, document.path.clone());
            if let Some(existing) = previous.remove(&key) {
                if existing.fingerprint == fingerprint {
                    indexed.push(existing);
                    continue;
                }
            }
            indexed.push(index_document(document, fingerprint, generation)?);
        }
    }
    indexed.sort_by(|left, right| (&left.scope, &left.path).cmp(&(&right.scope, &right.path)));
    let index = Index {
        schema_version: CURRENT_SCHEMA_VERSION,
        indexer_version: INDEXER_VERSION.into(),
        generation,
        parameters: parameters(),
        documents: indexed,
    };
    let parent = safe_path(root, ".kb/cache")?;
    fs::create_dir_all(&parent).map_err(|error| io("create cache directory", &parent, error))?;
    crate::operation::write_json(&path, &index)
}

pub(crate) fn search(
    root: &Path,
    scope: SearchScope,
    config: &EffectiveConfig,
    query: &str,
    limit: usize,
) -> Result<SearchGroup, KbError> {
    let path = safe_path(root, ".kb/cache/bm25.json")?;
    let index = read_index(&path)?;
    validate_current(root, scope, config, &index)?;
    Ok(rank(scope, query, limit, &index))
}

fn read_index(path: &Path) -> Result<Index, KbError> {
    let bytes = fs::read(path).map_err(|_| unavailable("BM25F index is missing."))?;
    let index: Index =
        serde_json::from_slice(&bytes).map_err(|_| unavailable("BM25F index is malformed."))?;
    if index.schema_version != CURRENT_SCHEMA_VERSION || index.indexer_version != INDEXER_VERSION {
        return Err(unavailable("BM25F index version is not supported."));
    }
    validate_index(&index)?;
    Ok(index)
}

fn validate_index(index: &Index) -> Result<(), KbError> {
    if index.parameters != parameters() {
        return Err(unavailable("BM25F index parameters are not supported."));
    }
    let mut documents = BTreeSet::new();
    for document in &index.documents {
        if document.scope == SearchScope::All
            || !documents.insert((document.scope, document.path.clone()))
            || document.chunks.iter().any(|chunk| {
                fields()
                    .iter()
                    .any(|field| !chunk.fields.contains_key(field))
            })
        {
            return Err(unavailable("BM25F index structure is invalid."));
        }
    }
    Ok(())
}

fn validate_current(
    root: &Path,
    scope: SearchScope,
    config: &EffectiveConfig,
    index: &Index,
) -> Result<(), KbError> {
    let actual = documents(root, scope, config)?
        .into_iter()
        .map(|document| (document.path.clone(), fingerprint(&document)))
        .collect::<BTreeMap<_, _>>();
    let stored = index
        .documents
        .iter()
        .filter(|document| document.scope == scope)
        .map(|document| (document.path.clone(), document.fingerprint.clone()))
        .collect::<BTreeMap<_, _>>();
    if actual != stored {
        return Err(unavailable("BM25F index is stale."));
    }
    Ok(())
}

fn index_document(
    document: Document,
    fingerprint: String,
    generation: u64,
) -> Result<IndexedDocument, KbError> {
    let extracted = extract_bytes(document.media, &document.bytes);
    let title = extracted.title.unwrap_or_else(|| document.title.clone());
    let (metadata_title, aliases, tags) = metadata(&document, &title)?;
    let mut chunks = extracted
        .blocks
        .into_iter()
        .map(|block| {
            chunk(
                &metadata_title,
                &aliases,
                &tags,
                ChunkSeed {
                    heading: block.heading,
                    line_start: block.line_start,
                    location: block.location,
                    text: block.text,
                    content_path: document.content_path.clone(),
                },
            )
        })
        .collect::<Vec<_>>();
    if let Some(annotation) = &document.annotation {
        chunks.extend(
            extract_bytes(kb_core::MediaType::Markdown, annotation.as_bytes())
                .blocks
                .into_iter()
                .map(|block| {
                    chunk(
                        &metadata_title,
                        &aliases,
                        &tags,
                        ChunkSeed {
                            heading: block.heading,
                            line_start: block.line_start,
                            location: block.location,
                            text: block.text,
                            content_path: document.path.clone(),
                        },
                    )
                }),
        );
    }
    Ok(IndexedDocument {
        scope: if document.source_uri.is_some() {
            SearchScope::Sources
        } else {
            SearchScope::Wiki
        },
        path: document.path,
        content_path: document.content_path,
        source_uri: document.source_uri,
        fingerprint,
        generation,
        title: metadata_title,
        chunks,
    })
}

fn metadata(
    document: &Document,
    fallback: &str,
) -> Result<(String, Vec<String>, Vec<String>), KbError> {
    if document.source_uri.is_some() {
        return Ok((fallback.into(), Vec::new(), Vec::new()));
    }
    let text = std::str::from_utf8(&document.bytes)
        .map_err(|error| KbError::invalid_config(document.path.as_str(), error.to_string()))?;
    let parsed = parse_okf(document.path.clone(), text);
    let mapping = parsed.frontmatter.as_ref().and_then(Value::as_mapping);
    Ok((
        mapping
            .and_then(|value| string(value, "title"))
            .unwrap_or_else(|| fallback.into()),
        mapping.map_or_else(Vec::new, |value| strings(value, "aliases")),
        mapping.map_or_else(Vec::new, |value| strings(value, "tags")),
    ))
}

fn string(mapping: &serde_yaml_ng::Mapping, key: &str) -> Option<String> {
    mapping
        .get(Value::String(key.into()))
        .and_then(Value::as_str)
        .map(Into::into)
}

fn strings(mapping: &serde_yaml_ng::Mapping, key: &str) -> Vec<String> {
    let Some(value) = mapping.get(Value::String(key.into())) else {
        return Vec::new();
    };
    if let Some(value) = value.as_str() {
        return vec![value.into()];
    }
    value.as_sequence().map_or_else(Vec::new, |items| {
        items
            .iter()
            .filter_map(Value::as_str)
            .map(Into::into)
            .collect()
    })
}

fn chunk(title: &str, aliases: &[String], tags: &[String], seed: ChunkSeed) -> Chunk {
    let mut fields = BTreeMap::new();
    for (field, value) in [
        (SearchField::Title, title.to_owned()),
        (SearchField::Aliases, aliases.join(" ")),
        (
            SearchField::Heading,
            seed.heading.clone().unwrap_or_default(),
        ),
        (SearchField::Tags, tags.join(" ")),
        (SearchField::Body, seed.text.clone()),
    ] {
        fields.insert(field, field_terms(&value));
    }
    Chunk {
        content_path: seed.content_path,
        heading: seed.heading,
        line_start: seed.line_start,
        location: seed.location,
        text: seed.text,
        fields,
    }
}

fn field_terms(value: &str) -> FieldTerms {
    let tokens = tokenize(value);
    let mut frequencies = BTreeMap::new();
    for token in &tokens {
        *frequencies.entry(token.clone()).or_insert(0) += 1;
    }
    FieldTerms {
        length: u32::try_from(tokens.len()).unwrap_or(u32::MAX),
        frequencies,
    }
}

pub(crate) fn tokenize(value: &str) -> Vec<String> {
    let mut output = Vec::new();
    let mut ascii = String::new();
    let mut unicode = Vec::new();
    let flush = |ascii: &mut String, unicode: &mut Vec<char>, output: &mut Vec<String>| {
        if !ascii.is_empty() {
            output.push(std::mem::take(ascii));
        }
        for width in 1..=3 {
            if unicode.len() >= width {
                for start in 0..=unicode.len() - width {
                    output.push(unicode[start..start + width].iter().collect());
                }
            }
        }
        unicode.clear();
    };
    for character in value.to_lowercase().chars() {
        if character.is_ascii_alphanumeric() {
            if !unicode.is_empty() {
                flush(&mut ascii, &mut unicode, &mut output);
            }
            ascii.push(character);
        } else if character.is_alphanumeric() {
            if !ascii.is_empty() {
                flush(&mut ascii, &mut unicode, &mut output);
            }
            unicode.push(character);
        } else {
            flush(&mut ascii, &mut unicode, &mut output);
        }
    }
    flush(&mut ascii, &mut unicode, &mut output);
    output
}

fn rank(scope: SearchScope, query: &str, limit: usize, index: &Index) -> SearchGroup {
    let query_terms = tokenize(query)
        .into_iter()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let chunks = index
        .documents
        .iter()
        .filter(|document| document.scope == scope)
        .flat_map(|document| document.chunks.iter())
        .collect::<Vec<_>>();
    let chunk_count = f64::from(u32::try_from(chunks.len()).unwrap_or(u32::MAX));
    let mut averages = BTreeMap::new();
    for field in fields() {
        let total = chunks
            .iter()
            .map(|chunk| chunk.fields[&field].length)
            .fold(0_u32, u32::saturating_add);
        averages.insert(
            field,
            if chunk_count == 0.0 {
                1.0
            } else {
                f64::from(total) / chunk_count
            },
        );
    }
    let mut hits = Vec::new();
    for document in index
        .documents
        .iter()
        .filter(|document| document.scope == scope)
    {
        for chunk in &document.chunks {
            let Some((score_micros, explanation)) =
                score_chunk(chunk, &chunks, chunk_count, &averages, &query_terms)
            else {
                continue;
            };
            hits.push((
                std::cmp::Reverse(score_micros),
                document,
                chunk,
                explanation,
            ));
        }
    }
    hits.sort_by(|left, right| {
        (&left.0, &left.1.path, &left.2.line_start).cmp(&(
            &right.0,
            &right.1.path,
            &right.2.line_start,
        ))
    });
    SearchGroup {
        scope,
        results: hits
            .into_iter()
            .take(limit)
            .map(|(score, document, chunk, explanation)| SearchHit {
                path: document.path.clone(),
                content_path: chunk.content_path.clone(),
                source_uri: document.source_uri.clone(),
                title: chunk
                    .heading
                    .clone()
                    .unwrap_or_else(|| document.title.clone()),
                heading: chunk.heading.clone(),
                line_start: chunk.line_start,
                location: chunk.location.clone(),
                snippet: chunk.text.chars().take(240).collect(),
                match_count: 0,
                backend: Some(SearchBackend::Bm25f),
                score_micros: Some(score.0),
                explanation: Some(explanation),
            })
            .collect(),
    }
}

fn score_chunk(
    chunk: &Chunk,
    chunks: &[&Chunk],
    chunk_count: f64,
    averages: &BTreeMap<SearchField, f64>,
    query_terms: &[String],
) -> Option<(u64, SearchExplanation)> {
    let mut contributions = BTreeMap::<SearchField, (f64, BTreeSet<String>)>::new();
    let mut total_score = 0.0;
    for term in query_terms {
        let frequency = chunks
            .iter()
            .filter(|candidate| {
                candidate
                    .fields
                    .values()
                    .any(|field| field.frequencies.contains_key(term))
            })
            .count();
        let document_frequency = f64::from(u32::try_from(frequency).unwrap_or(u32::MAX));
        if document_frequency == 0.0 {
            continue;
        }
        let idf =
            (1.0 + (chunk_count - document_frequency + 0.5) / (document_frequency + 0.5)).ln();
        let mut weighted = Vec::new();
        let mut total_tf = 0.0;
        for field in fields() {
            let data = &chunk.fields[&field];
            let term_frequency = f64::from(*data.frequencies.get(term).unwrap_or(&0));
            if term_frequency == 0.0 {
                continue;
            }
            let normalized = weight(field) * term_frequency
                / (1.0 - B + B * f64::from(data.length) / averages[&field].max(1.0));
            total_tf += normalized;
            weighted.push((field, normalized));
        }
        if total_tf == 0.0 {
            continue;
        }
        let term_score = idf * (K1 + 1.0) * total_tf / (K1 + total_tf);
        total_score += term_score;
        for (field, value) in weighted {
            let entry = contributions.entry(field).or_default();
            entry.0 += term_score * value / total_tf;
            entry.1.insert(term.clone());
        }
    }
    if total_score == 0.0 {
        return None;
    }
    Some((
        micros(total_score),
        SearchExplanation {
            query_terms: query_terms.to_vec(),
            fields: contributions
                .into_iter()
                .map(|(field, (value, terms))| SearchFieldContribution {
                    field,
                    terms: terms.into_iter().collect(),
                    score_micros: micros(value),
                })
                .collect(),
        },
    ))
}

fn fields() -> [SearchField; 5] {
    [
        SearchField::Title,
        SearchField::Aliases,
        SearchField::Heading,
        SearchField::Tags,
        SearchField::Body,
    ]
}

const fn weight(field: SearchField) -> f64 {
    match field {
        SearchField::Title => 3.0,
        SearchField::Aliases | SearchField::Heading => 2.5,
        SearchField::Tags => 2.0,
        SearchField::Body => 1.0,
    }
}

fn parameters() -> Parameters {
    Parameters {
        k1_milli: 1_200,
        b_milli: 750,
        field_weights_milli: fields()
            .into_iter()
            .map(|field| {
                let value = match field {
                    SearchField::Title => 3_000,
                    SearchField::Aliases | SearchField::Heading => 2_500,
                    SearchField::Tags => 2_000,
                    SearchField::Body => 1_000,
                };
                (field, value)
            })
            .collect(),
    }
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn micros(value: f64) -> u64 {
    (value * 1_000_000.0).round().max(0.0) as u64
}

fn fingerprint(document: &Document) -> String {
    let mut bytes = document.bytes.clone();
    bytes.push(0);
    if let Some(annotation) = &document.annotation {
        bytes.extend_from_slice(annotation.as_bytes());
    }
    hash(&bytes)
}

fn unavailable(message: &str) -> KbError {
    KbError::new(
        ErrorCode::IndexStale,
        message,
        true,
        "Run kb cache rebuild or omit --strict-backend to use direct search.",
    )
}

#[cfg(test)]
mod tests {
    use super::tokenize;
    #[test]
    fn tokenizes_ascii_words_and_cjk_ngrams() {
        assert_eq!(tokenize("Local-first KB"), ["local", "first", "kb"]);
        let terms = tokenize("知识库");
        for expected in ["知", "识", "库", "知识", "识库", "知识库"] {
            assert!(terms.iter().any(|term| term == expected));
        }
    }
}
