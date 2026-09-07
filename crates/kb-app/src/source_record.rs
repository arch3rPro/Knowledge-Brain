use crate::source_io::{Budget, hash, list_files, safe_path};
use kb_core::{
    EffectiveConfig, ExtractionStatus, KbError, MediaType, PortableRelativePath, SourceId,
    SourceVersion,
};
use serde::{Deserialize, Serialize};
use serde_yaml_ng::Value;
use std::{collections::BTreeMap, path::Path};
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceRecord {
    pub source: SourceVersion,
    pub title: String,
    pub size: u64,
    pub media_type: MediaType,
    pub extraction_status: ExtractionStatus,
    pub present: bool,
    pub captured_at: String,
    pub versions: Vec<SourceVersion>,
}
pub(crate) struct StoredRecord {
    pub record: SourceRecord,
    pub path: PortableRelativePath,
    pub markdown: String,
}
pub(crate) fn record_path_for(id: &SourceId) -> Result<PortableRelativePath, KbError> {
    let digest = hash(id.logical_uri().as_bytes());
    PortableRelativePath::parse(&format!(
        "Wiki/external-sources/records/{}/{}.md",
        &digest[..2],
        digest
    ))
}
pub(crate) fn render(record: &SourceRecord, previous: Option<&str>) -> Result<String, KbError> {
    let (mut meta, body) = if let Some(text) = previous {
        split(text)?
    } else {
        (
            Value::Mapping(serde_yaml_ng::Mapping::default()),
            format!(
                "# {}\n\nSource material retained for citation.\n",
                record.title
            ),
        )
    };
    meta["type"] = Value::String("Reference".into());
    meta["title"] = Value::String(record.title.clone());
    meta["resource"] = Value::String(record.source.source.logical_uri());
    meta["generated"]=serde_yaml_ng::to_value(serde_json::json!({"by":format!("process:knowledge-brain/{}",env!("CARGO_PKG_VERSION")),"at":record.captured_at})).map_err(|e| yaml(&e))?;
    if meta.get("kb").is_none() {
        meta["kb"] = Value::Mapping(serde_yaml_ng::Mapping::default());
    }
    if !meta["kb"].is_mapping() {
        return Err(KbError::invalid_config(
            "source record",
            "kb must be a mapping",
        ));
    }
    meta["kb"]["source"] = serde_yaml_ng::to_value(record).map_err(|e| yaml(&e))?;
    Ok(format!(
        "---\n{}---\n\n{}",
        serde_yaml_ng::to_string(&meta).map_err(|e| yaml(&e))?,
        body
    ))
}
pub(crate) fn parse(text: &str) -> Result<SourceRecord, KbError> {
    let (meta, _) = split(text)?;
    serde_yaml_ng::from_value(meta["kb"]["source"].clone()).map_err(|e| yaml(&e))
}
fn split(text: &str) -> Result<(Value, String), KbError> {
    let normalized = text.replace("\r\n", "\n");
    let rest = normalized
        .strip_prefix("---\n")
        .ok_or_else(|| KbError::invalid_config("source record", "frontmatter is required"))?;
    let (yaml_text, body) = rest
        .split_once("\n---\n")
        .ok_or_else(|| KbError::invalid_config("source record", "unclosed frontmatter"))?;
    let meta: Value = serde_yaml_ng::from_str(yaml_text).map_err(|e| yaml(&e))?;
    if !meta.is_mapping() {
        return Err(KbError::invalid_config(
            "source record",
            "frontmatter must be a mapping",
        ));
    }
    Ok((meta, body.trim_start_matches('\n').to_owned()))
}
fn yaml(e: &serde_yaml_ng::Error) -> KbError {
    KbError::invalid_config("source record", e.to_string())
}
pub(crate) fn inventory(
    root: &Path,
    config: &EffectiveConfig,
    b: &mut Budget,
) -> Result<BTreeMap<SourceId, StoredRecord>, KbError> {
    let mut records = BTreeMap::new();
    for path in list_files(root, "Wiki/external-sources/records", b)? {
        if !Path::new(path.as_str())
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("md"))
        {
            continue;
        }
        let bytes = b.read(&safe_path(root, path.as_str())?)?;
        let text = String::from_utf8(bytes)
            .map_err(|e| KbError::invalid_config(path.as_str(), e.to_string()))?;
        let r = parse(&text).map_err(|e| KbError::invalid_config(path.as_str(), e.message))?;
        if r.versions.iter().any(|v| v.source != r.source.source)
            || record_path_for(&r.source.source)? != path
            || records.contains_key(&r.source.source)
        {
            return Err(KbError::invalid_config(
                path.as_str(),
                "record identity, history or path mismatch",
            ));
        }
        records.insert(
            r.source.source.clone(),
            StoredRecord {
                record: r,
                path,
                markdown: text,
            },
        );
    }
    let _ = config;
    Ok(records)
}
