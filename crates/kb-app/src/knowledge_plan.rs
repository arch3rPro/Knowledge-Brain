use std::{collections::BTreeMap, fs, path::Path};

use kb_core::{
    CURRENT_SCHEMA_VERSION, EffectiveConfig, KbError, KnowledgeChangeRequest, KnowledgePlan,
    KnowledgePlanRequest, KnowledgeWrite, OkfSeverity, OperationId, OperationKind,
    PortableRelativePath, parse_okf, validate_okf,
};
use serde_yaml_ng::Value;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

use crate::{
    UserPaths,
    managed_markdown::{IndexEntry, LogEntry, render_index, render_log},
    operation::{operation_directory, write_json},
    source_io::{Budget, hash, list_files, read_config_hash, safe_path},
    source_record,
};

/// Validate a structured suggestion and persist an inspectable, read-only plan.
///
/// # Errors
///
/// Rejects stale inputs, invalid managed Markdown, unresolved exact source
/// versions, corrupt managed markers, and configured read limits.
pub fn create_knowledge_plan(
    root: &Path,
    user_paths: &UserPaths,
    config: &EffectiveConfig,
    request: KnowledgePlanRequest,
    now: OffsetDateTime,
) -> Result<KnowledgePlan, KbError> {
    request.validate(
        config.limits.max_files_per_review.value,
        config.limits.max_file_bytes.value,
    )?;
    let mut budget = Budget::new(config);
    let mut documents = load_documents(root, &mut budget)?;
    let source_versions = available_source_versions(root, config, &mut budget)?;
    let mut exact_sources = BTreeMap::new();
    let mut writes = Vec::new();

    for change in &request.changes {
        let full_path = PortableRelativePath::parse(&format!("Wiki/{}", change.path.as_str()))?;
        let target = safe_path(root, full_path.as_str())?;
        let current = if target.exists() {
            Some(budget.read(&target)?)
        } else {
            None
        };
        if current.as_ref().map(|bytes| hash(bytes)) != change.before_sha256 {
            return Err(stale(format!(
                "{} changed before the plan was created.",
                full_path.as_str()
            )));
        }
        validate_candidate(
            &full_path,
            change,
            now,
            &source_versions,
            &mut exact_sources,
        )?;
        documents.insert(change.path.clone(), change.content.clone());
        writes.push(KnowledgeWrite {
            path: full_path,
            before_sha256: change.before_sha256.clone(),
            content: change.content.clone(),
        });
    }

    let index_path = PortableRelativePath::parse("Wiki/index.md")?;
    let log_path = PortableRelativePath::parse("Wiki/log.md")?;
    let index_before = budget.read(&safe_path(root, index_path.as_str())?)?;
    let log_before = budget.read(&safe_path(root, log_path.as_str())?)?;
    let (research, articles) = index_entries(&documents, now)?;
    let index_after = render_index(utf8(&index_path, &index_before)?, &research, &articles)?;
    let date = now.date().to_string();
    let log_entries = request
        .changes
        .iter()
        .map(|change| LogEntry {
            path: change.path.as_str().to_owned(),
            title: candidate_title(change).expect("candidate validation requires title"),
            summary: change.summary.clone(),
            creation: change.before_sha256.is_none(),
        })
        .collect::<Vec<_>>();
    let log_after = render_log(utf8(&log_path, &log_before)?, &date, &log_entries)?;
    writes.push(KnowledgeWrite {
        path: index_path,
        before_sha256: Some(hash(&index_before)),
        content: index_after,
    });
    writes.push(KnowledgeWrite {
        path: log_path,
        before_sha256: Some(hash(&log_before)),
        content: log_after,
    });
    writes.sort_by(|left, right| left.path.cmp(&right.path));

    let plan = KnowledgePlan {
        schema_version: CURRENT_SCHEMA_VERSION,
        operation_id: OperationId::new(),
        kind: OperationKind::SaveKnowledge,
        vault_id: config.vault_id,
        target: root.to_path_buf(),
        changes: request.changes,
        source_versions: exact_sources.into_keys().collect(),
        admission_sha256: hash(&budget.read(&safe_path(root, "admission.yml")?)?),
        config_sha256: read_config_hash(config)?,
        diff: render_diff(&writes, root, &mut budget)?,
        writes,
        created_at: now
            .format(&Rfc3339)
            .map_err(|error| KbError::invalid_config("plan creation time", error.to_string()))?,
        app_version: env!("CARGO_PKG_VERSION").into(),
    };
    persist_plan(user_paths, &plan)?;
    Ok(plan)
}

fn load_documents(
    root: &Path,
    budget: &mut Budget,
) -> Result<BTreeMap<PortableRelativePath, String>, KbError> {
    let mut documents = BTreeMap::new();
    for directory in ["Wiki/research", "Wiki/articles"] {
        for path in list_files(root, directory, budget)? {
            if !Path::new(path.as_str())
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("md"))
            {
                continue;
            }
            let relative = path
                .as_str()
                .strip_prefix("Wiki/")
                .expect("paths were listed below Wiki");
            let bytes = budget.read(&safe_path(root, path.as_str())?)?;
            documents.insert(
                PortableRelativePath::parse(relative)?,
                utf8(&path, &bytes)?.to_owned(),
            );
        }
    }
    Ok(documents)
}

fn available_source_versions(
    root: &Path,
    config: &EffectiveConfig,
    budget: &mut Budget,
) -> Result<BTreeMap<String, ()>, KbError> {
    let mut versions = BTreeMap::new();
    for (_, stored) in source_record::inventory(root, config, budget)? {
        for version in stored.record.versions {
            versions.insert(version.exact_uri(), ());
        }
    }
    Ok(versions)
}

fn validate_candidate(
    path: &PortableRelativePath,
    change: &KnowledgeChangeRequest,
    now: OffsetDateTime,
    available_sources: &BTreeMap<String, ()>,
    exact_sources: &mut BTreeMap<String, ()>,
) -> Result<(), KbError> {
    let document = parse_okf(path.clone(), &change.content);
    let findings = validate_okf(&document, now);
    if !document.managed
        || findings.iter().any(|finding| {
            finding.severity == OkfSeverity::Error || finding.code == "stale_document"
        })
    {
        return Err(KbError::invalid_config(
            path.as_str(),
            "knowledge target must satisfy the managed OKF Producer Profile and not be stale",
        )
        .with_details(serde_json::json!({"findings": findings})));
    }
    for source in document.sources {
        if source.resource.starts_with("kb-source://") {
            if !available_sources.contains_key(&source.resource) {
                return Err(KbError::invalid_config(
                    path.as_str(),
                    format!("exact source version does not exist: {}", source.resource),
                ));
            }
            exact_sources.insert(source.resource, ());
        }
    }
    Ok(())
}

fn index_entries(
    documents: &BTreeMap<PortableRelativePath, String>,
    now: OffsetDateTime,
) -> Result<(Vec<IndexEntry>, Vec<IndexEntry>), KbError> {
    let mut research = Vec::new();
    let mut articles = Vec::new();
    for (path, content) in documents {
        let full_path = PortableRelativePath::parse(&format!("Wiki/{}", path.as_str()))?;
        let document = parse_okf(full_path, content);
        if !document.managed {
            continue;
        }
        let findings = validate_okf(&document, now);
        if findings
            .iter()
            .any(|finding| finding.severity == OkfSeverity::Error)
        {
            return Err(KbError::invalid_config(
                path.as_str(),
                "existing managed concept is invalid; run kb lint",
            ));
        }
        let mapping = document
            .frontmatter
            .as_ref()
            .and_then(Value::as_mapping)
            .expect("managed validation requires frontmatter");
        let entry = IndexEntry {
            path: path.as_str().to_owned(),
            title: string_field(mapping, "title").expect("managed validation requires title"),
            description: string_field(mapping, "description"),
        };
        if path.as_str().starts_with("research/") {
            research.push(entry);
        } else {
            articles.push(entry);
        }
    }
    Ok((research, articles))
}

fn candidate_title(change: &KnowledgeChangeRequest) -> Option<String> {
    let path = PortableRelativePath::parse(&format!("Wiki/{}", change.path.as_str())).ok()?;
    let document = parse_okf(path, &change.content);
    let mapping = document.frontmatter.as_ref()?.as_mapping()?;
    string_field(mapping, "title")
}

fn string_field(mapping: &serde_yaml_ng::Mapping, key: &str) -> Option<String> {
    mapping
        .get(Value::String(key.to_owned()))
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}

fn render_diff(
    writes: &[KnowledgeWrite],
    root: &Path,
    budget: &mut Budget,
) -> Result<String, KbError> {
    let mut output = String::new();
    for write in writes {
        let path = safe_path(root, write.path.as_str())?;
        let before = if path.exists() {
            utf8(&write.path, &budget.read(&path)?)?.to_owned()
        } else {
            String::new()
        };
        output.push_str("--- ");
        output.push_str(if write.before_sha256.is_some() {
            write.path.as_str()
        } else {
            "/dev/null"
        });
        output.push_str("\n+++ ");
        output.push_str(write.path.as_str());
        output.push_str("\n@@ complete file @@\n");
        for line in before.lines() {
            output.push('-');
            output.push_str(line);
            output.push('\n');
        }
        for line in write.content.lines() {
            output.push('+');
            output.push_str(line);
            output.push('\n');
        }
    }
    Ok(output)
}

fn persist_plan(user_paths: &UserPaths, plan: &KnowledgePlan) -> Result<(), KbError> {
    let directory = operation_directory(user_paths, plan.operation_id);
    fs::create_dir_all(&directory).map_err(|error| {
        KbError::io_failure(
            "create operation directory",
            directory.display().to_string(),
            error.to_string(),
        )
    })?;
    write_json(&directory.join("plan.json"), plan)?;
    let bytes = serde_json::to_vec(plan)
        .map_err(|error| KbError::invalid_config("knowledge plan", error.to_string()))?;
    write_json(&directory.join("plan.sha256"), &hash(&bytes))
}

fn utf8<'a>(path: &PortableRelativePath, bytes: &'a [u8]) -> Result<&'a str, KbError> {
    std::str::from_utf8(bytes)
        .map_err(|error| KbError::invalid_config(path.as_str(), error.to_string()))
}

fn stale(message: impl Into<String>) -> KbError {
    KbError::new(
        kb_core::ErrorCode::PlanStale,
        message,
        false,
        "Read the current target and create a new knowledge request.",
    )
}
