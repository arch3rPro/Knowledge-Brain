use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

use kb_core::{
    CURRENT_SCHEMA_VERSION, EffectiveConfig, ErrorCode, KbError, KnowledgeChangeKind,
    KnowledgeChangeRequest, KnowledgeOrigin, KnowledgePlan, KnowledgePlanRequest, KnowledgeWrite,
    OkfSeverity, OperationId, OperationKind, PortableRelativePath, knowledge_origin, parse_okf,
    validate_okf,
};
use serde_yaml_ng::Value;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

use crate::{
    UserPaths,
    managed_markdown::{
        IndexEntry, LogAction, LogEntry, normalize_title, render_index, render_log,
    },
    operation::{create_private_directory_all, operation_directory, write_json},
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
    let prepared = ChangePreparer::new(root, now, &source_versions, &mut budget, &mut documents)
        .prepare(&request.changes)?;
    let exact_sources = prepared.exact_sources;
    let mut writes = prepared.writes;
    let log_entries = prepared.log_entries;

    let index_path = PortableRelativePath::parse("Wiki/index.md")?;
    let log_path = PortableRelativePath::parse("Wiki/log.md")?;
    let index_before = budget.read(&safe_path(root, index_path.as_str())?)?;
    let log_before = budget.read(&safe_path(root, log_path.as_str())?)?;
    let (research, articles) = index_entries(&documents, now)?;
    ensure_unique_titles("research", &research)?;
    ensure_unique_titles("articles", &articles)?;
    let index_after = render_index(utf8(&index_path, &index_before)?, &research, &articles)?;
    let date = now.date().to_string();
    let log_after = render_log(utf8(&log_path, &log_before)?, &date, &log_entries)?;
    writes.push(KnowledgeWrite {
        path: index_path,
        before_sha256: Some(hash(&index_before)),
        delete: false,
        content: index_after,
    });
    writes.push(KnowledgeWrite {
        path: log_path,
        before_sha256: Some(hash(&log_before)),
        delete: false,
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
        source_versions: exact_sources.into_iter().collect(),
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

struct PreparedChanges {
    exact_sources: BTreeSet<String>,
    writes: Vec<KnowledgeWrite>,
    log_entries: Vec<LogEntry>,
}

struct ChangePreparer<'a> {
    root: &'a Path,
    now: OffsetDateTime,
    source_versions: &'a BTreeSet<String>,
    budget: &'a mut Budget,
    documents: &'a mut BTreeMap<PortableRelativePath, String>,
    prepared: PreparedChanges,
}

impl<'a> ChangePreparer<'a> {
    fn new(
        root: &'a Path,
        now: OffsetDateTime,
        source_versions: &'a BTreeSet<String>,
        budget: &'a mut Budget,
        documents: &'a mut BTreeMap<PortableRelativePath, String>,
    ) -> Self {
        Self {
            root,
            now,
            source_versions,
            budget,
            documents,
            prepared: PreparedChanges {
                exact_sources: BTreeSet::new(),
                writes: Vec::new(),
                log_entries: Vec::new(),
            },
        }
    }

    fn prepare(mut self, changes: &[KnowledgeChangeRequest]) -> Result<PreparedChanges, KbError> {
        for change in changes {
            match change.kind {
                KnowledgeChangeKind::Upsert => self.upsert(change)?,
                KnowledgeChangeKind::Delete => self.delete(change)?,
                KnowledgeChangeKind::Move => self.move_page(change)?,
            }
        }
        Ok(self.prepared)
    }

    fn upsert(&mut self, change: &KnowledgeChangeRequest) -> Result<(), KbError> {
        let full_path = wiki_path(&change.path)?;
        let current = read_optional(self.root, &full_path, self.budget)?;
        ensure_observed(
            &full_path,
            current.as_deref(),
            change.before_sha256.as_deref(),
        )?;
        validate_candidate(
            &full_path,
            change,
            self.now,
            self.source_versions,
            &mut self.prepared.exact_sources,
        )?;
        self.documents
            .insert(change.path.clone(), change.content.clone());
        self.prepared.writes.push(KnowledgeWrite {
            path: full_path,
            before_sha256: change.before_sha256.clone(),
            delete: false,
            content: change.content.clone(),
        });
        self.prepared.log_entries.push(LogEntry {
            path: change.path.as_str().to_owned(),
            title: candidate_title(change)?,
            summary: change.summary.clone(),
            action: if change.before_sha256.is_none() {
                LogAction::Creation
            } else {
                LogAction::Update
            },
            from_path: None,
        });
        Ok(())
    }

    fn delete(&mut self, change: &KnowledgeChangeRequest) -> Result<(), KbError> {
        let full_path = wiki_path(&change.path)?;
        let current = read_required(self.root, &full_path, self.budget)?;
        ensure_observed(&full_path, Some(&current), change.before_sha256.as_deref())?;
        let title = existing_managed_title(&change.path, utf8(&full_path, &current)?)?;
        self.documents.remove(&change.path);
        self.prepared.writes.push(KnowledgeWrite {
            path: full_path,
            before_sha256: change.before_sha256.clone(),
            delete: true,
            content: String::new(),
        });
        self.prepared.log_entries.push(LogEntry {
            path: change.path.as_str().to_owned(),
            title,
            summary: change.summary.clone(),
            action: LogAction::Deletion,
            from_path: None,
        });
        Ok(())
    }

    fn move_page(&mut self, change: &KnowledgeChangeRequest) -> Result<(), KbError> {
        let from_path = change.from_path.as_ref().ok_or_else(|| {
            KbError::invalid_config("knowledge request move", "from_path is required")
        })?;
        let full_source = wiki_path(from_path)?;
        let source = read_required(self.root, &full_source, self.budget)?;
        ensure_observed(&full_source, Some(&source), change.before_sha256.as_deref())?;
        existing_managed_title(from_path, utf8(&full_source, &source)?)?;
        let full_target = wiki_path(&change.path)?;
        let target = read_optional(self.root, &full_target, self.budget)?;
        ensure_observed(&full_target, target.as_deref(), None)?;
        validate_candidate(
            &full_target,
            change,
            self.now,
            self.source_versions,
            &mut self.prepared.exact_sources,
        )?;
        self.documents.remove(from_path);
        self.documents
            .insert(change.path.clone(), change.content.clone());
        self.prepared.writes.push(KnowledgeWrite {
            path: full_target,
            before_sha256: None,
            delete: false,
            content: change.content.clone(),
        });
        self.prepared.writes.push(KnowledgeWrite {
            path: full_source,
            before_sha256: change.before_sha256.clone(),
            delete: true,
            content: String::new(),
        });
        self.prepared.log_entries.push(LogEntry {
            path: change.path.as_str().to_owned(),
            title: candidate_title(change)?,
            summary: change.summary.clone(),
            action: LogAction::Move,
            from_path: Some(from_path.as_str().to_owned()),
        });
        Ok(())
    }
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
            let relative = path.as_str().strip_prefix("Wiki/").ok_or_else(|| {
                KbError::invalid_config(path.as_str(), "Wiki path lost its expected prefix")
            })?;
            let bytes = budget.read(&safe_path(root, path.as_str())?)?;
            documents.insert(
                PortableRelativePath::parse(relative)?,
                utf8(&path, &bytes)?.to_owned(),
            );
        }
    }
    Ok(documents)
}

fn wiki_path(path: &PortableRelativePath) -> Result<PortableRelativePath, KbError> {
    PortableRelativePath::parse(&format!("Wiki/{}", path.as_str()))
}

fn read_optional(
    root: &Path,
    path: &PortableRelativePath,
    budget: &mut Budget,
) -> Result<Option<Vec<u8>>, KbError> {
    let target = safe_path(root, path.as_str())?;
    if target.exists() {
        budget.read(&target).map(Some)
    } else {
        Ok(None)
    }
}

fn read_required(
    root: &Path,
    path: &PortableRelativePath,
    budget: &mut Budget,
) -> Result<Vec<u8>, KbError> {
    read_optional(root, path, budget)?.ok_or_else(|| {
        stale(format!(
            "{} does not exist at the observed source path.",
            path.as_str()
        ))
    })
}

fn ensure_observed(
    path: &PortableRelativePath,
    current: Option<&[u8]>,
    expected: Option<&str>,
) -> Result<(), KbError> {
    if current.map(hash).as_deref() != expected {
        return Err(stale(format!(
            "{} changed before the plan was created.",
            path.as_str()
        )));
    }
    Ok(())
}

fn available_source_versions(
    root: &Path,
    config: &EffectiveConfig,
    budget: &mut Budget,
) -> Result<BTreeSet<String>, KbError> {
    let mut versions = BTreeSet::new();
    for (_, stored) in source_record::inventory(root, config, budget)? {
        for version in stored.record.versions {
            versions.insert(version.exact_uri());
        }
    }
    Ok(versions)
}

fn validate_candidate(
    path: &PortableRelativePath,
    change: &KnowledgeChangeRequest,
    now: OffsetDateTime,
    available_sources: &BTreeSet<String>,
    exact_sources: &mut BTreeSet<String>,
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
    let external_research = knowledge_origin(&document) == Some(KnowledgeOrigin::ExternalResearch);
    let mut has_exact_source = false;
    for source in document.sources {
        if source.resource.starts_with("kb-source://") {
            if !available_sources.contains(&source.resource) {
                return Err(KbError::invalid_config(
                    path.as_str(),
                    format!("exact source version does not exist: {}", source.resource),
                ));
            }
            has_exact_source = true;
            exact_sources.insert(source.resource);
        }
    }
    if external_research && !has_exact_source {
        return Err(KbError::new(
            ErrorCode::InvalidConfig,
            format!(
                "External research {} has no admitted exact source version.",
                path.as_str()
            ),
            false,
            "Save the source from an enabled admission directory, then reference its exact kb-source URI.",
        )
        .with_details(serde_json::json!({
            "finding_code": "source_admission_required",
            "path": path.as_str(),
            "origin": "external_research",
        })));
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
            .ok_or_else(|| KbError::invalid_config(path.as_str(), "managed frontmatter missing"))?;
        let entry = IndexEntry {
            path: path.as_str().to_owned(),
            title: string_field(mapping, "title")
                .ok_or_else(|| KbError::invalid_config(path.as_str(), "managed title missing"))?,
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

fn ensure_unique_titles(partition: &str, entries: &[IndexEntry]) -> Result<(), KbError> {
    let mut titles: BTreeMap<String, Vec<&IndexEntry>> = BTreeMap::new();
    for entry in entries {
        titles
            .entry(normalize_title(&entry.title))
            .or_default()
            .push(entry);
    }
    let Some((normalized_title, duplicates)) =
        titles.into_iter().find(|(_, entries)| entries.len() > 1)
    else {
        return Ok(());
    };
    let title = duplicates[0].title.clone();
    let paths = duplicates
        .iter()
        .map(|entry| entry.path.clone())
        .collect::<Vec<_>>();
    Err(KbError::new(
        ErrorCode::InvalidConfig,
        format!("Duplicate Wiki title {title:?} in the {partition} index partition."),
        false,
        "Give every Wiki concept in this index partition a distinct, subject-specific title.",
    )
    .with_details(serde_json::json!({
        "finding_code": "duplicate_title",
        "partition": partition,
        "normalized_title": normalized_title,
        "title": title,
        "paths": paths,
    })))
}

fn candidate_title(change: &KnowledgeChangeRequest) -> Result<String, KbError> {
    content_title(&change.path, &change.content)
}

fn content_title(path: &PortableRelativePath, content: &str) -> Result<String, KbError> {
    let document = parse_okf(wiki_path(path)?, content);
    let mapping = document
        .frontmatter
        .as_ref()
        .and_then(Value::as_mapping)
        .ok_or_else(|| KbError::invalid_config(path.as_str(), "frontmatter missing"))?;
    string_field(mapping, "title")
        .ok_or_else(|| KbError::invalid_config(path.as_str(), "title missing"))
}

fn existing_managed_title(path: &PortableRelativePath, content: &str) -> Result<String, KbError> {
    let document = parse_okf(wiki_path(path)?, content);
    if !document.managed {
        return Err(KbError::invalid_config(
            path.as_str(),
            "delete and move only accept managed Wiki concepts",
        ));
    }
    let mapping = document
        .frontmatter
        .as_ref()
        .and_then(Value::as_mapping)
        .ok_or_else(|| KbError::invalid_config(path.as_str(), "frontmatter missing"))?;
    string_field(mapping, "title")
        .ok_or_else(|| KbError::invalid_config(path.as_str(), "managed title missing"))
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
        output.push_str(if write.delete {
            "/dev/null"
        } else {
            write.path.as_str()
        });
        output.push_str("\n@@ complete file @@\n");
        for line in before.lines() {
            output.push('-');
            output.push_str(line);
            output.push('\n');
        }
        if !write.delete {
            for line in write.content.lines() {
                output.push('+');
                output.push_str(line);
                output.push('\n');
            }
        }
    }
    Ok(output)
}

fn persist_plan(user_paths: &UserPaths, plan: &KnowledgePlan) -> Result<(), KbError> {
    let directory = operation_directory(user_paths, plan.operation_id);
    create_private_directory_all(&directory)?;
    write_json(&directory.join("plan.json"), plan)?;
    let bytes = serde_json::to_vec(plan)
        .map_err(|error| KbError::invalid_config("knowledge plan", error.to_string()))?;
    write_json(&directory.join("plan.sha256"), &hash(&bytes))?;
    crate::operation_events::record_operation_event_now(
        user_paths,
        plan.operation_id,
        kb_core::OperationEventKind::Planned,
        Some((0, plan.writes.len() as u64)),
        "Knowledge save plan is ready for review.",
    )?;
    Ok(())
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
