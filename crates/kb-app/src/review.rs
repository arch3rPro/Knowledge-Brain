use crate::{
    UserPaths, discover_sources, extract_bytes, load_admission,
    operation::{operation_directory, write_json},
    source_io::{Budget, hash, read_config_hash, safe_path},
    source_plan::{
        CaptureKind, ChangeKind, RecordWrite, ReviewReport, SourceCapturePlan, SourceChange,
        SourceInput,
    },
    source_record::{self, SourceRecord},
};
use kb_core::{
    CURRENT_SCHEMA_VERSION, EffectiveConfig, KbError, OperationId, PortableRelativePath,
};
use std::{collections::BTreeMap, fs, path::Path};
/// Review admitted sources and persist a plan without writing knowledge.
/// # Errors
/// Returns admission, source, limit, record or plan-persistence errors.
pub fn review_sources(
    root: &Path,
    paths: &UserPaths,
    config: &EffectiveConfig,
) -> Result<ReviewReport, KbError> {
    let admission = load_admission(root)?;
    let snapshot = discover_sources(root, &admission, config)?;
    let mut budget = Budget::new(config);
    let previous = source_record::inventory(root, config, &mut budget)?;
    let current = snapshot
        .sources
        .iter()
        .map(|s| (s.version.source.clone(), s))
        .collect::<BTreeMap<_, _>>();
    let enabled = admission
        .directories
        .iter()
        .filter(|e| e.enabled)
        .map(|e| (e.id.as_str(), e.path.as_str()))
        .collect::<BTreeMap<_, _>>();
    let mut changes = Vec::new();
    let mut inputs = Vec::new();
    let mut writes = Vec::new();
    let mut unchanged = 0;
    let at = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .map_err(|e| KbError::invalid_config("clock", e.to_string()))?;
    let deleted = missing_sources(
        root,
        &admission,
        config,
        &previous,
        &current,
        &snapshot.skipped,
    )?;
    for (id, item) in &current {
        let old = previous.get(id);
        if old.is_some_and(|r| r.record.present && r.record.source == item.version) {
            unchanged += 1;
            continue;
        }
        let extraction = extract_bytes(item.media_type, &item.bytes);
        let record = captured_record(item, old, &at);
        let path = source_record::record_path_for(id)?;
        writes.push(RecordWrite {
            path,
            before_sha256: old.map(|r| hash(r.markdown.as_bytes())),
            content: source_record::render(&record, old.map(|r| r.markdown.as_str()))?,
        });
        inputs.push(plan_input(&record, enabled[id.admission_id()])?);
        changes.push(SourceChange {
            kind: if old.is_some() {
                ChangeKind::Modified
            } else {
                ChangeKind::Added
            },
            source: id.clone(),
            previous_sha256: old.map(|r| r.record.source.sha256().into()),
            current_sha256: Some(item.version.sha256().into()),
            possible_move_from: deleted
                .iter()
                .filter(|(_, r)| r.record.source.sha256() == item.version.sha256())
                .map(|(id, _)| (*id).clone())
                .collect(),
            extraction: Some(extraction),
        });
    }
    for (id, old) in deleted {
        let mut record = old.record.clone();
        record.present = false;
        writes.push(RecordWrite {
            path: old.path.clone(),
            before_sha256: Some(hash(old.markdown.as_bytes())),
            content: source_record::render(&record, Some(&old.markdown))?,
        });
        inputs.push(plan_input(&record, enabled[id.admission_id()])?);
        changes.push(SourceChange {
            kind: ChangeKind::Deleted,
            source: id.clone(),
            previous_sha256: Some(record.source.sha256().into()),
            current_sha256: None,
            possible_move_from: Vec::new(),
            extraction: None,
        });
    }
    changes.sort_by_key(|c| c.source.logical_uri());
    let operation_id = persist_plan(root, paths, config, inputs, writes, at)?;
    Ok(ReviewReport {
        schema_version: CURRENT_SCHEMA_VERSION,
        vault_id: config.vault_id,
        operation_id,
        changes,
        unchanged,
        skipped: snapshot.skipped,
    })
}
fn plan_input(record: &SourceRecord, topic: &str) -> Result<SourceInput, KbError> {
    Ok(SourceInput {
        version: record.source.clone(),
        relative_path: PortableRelativePath::parse(&format!(
            "{}/{}",
            topic,
            record.source.source.relative_path().as_str()
        ))?,
        media_type: record.media_type,
        present: record.present,
    })
}
fn captured_record(
    item: &crate::DiscoveredSource,
    old: Option<&source_record::StoredRecord>,
    at: &str,
) -> SourceRecord {
    let id = &item.version.source;
    let mut versions = old.map_or_else(Vec::new, |r| r.record.versions.clone());
    if let Some(r) = old {
        if !versions.contains(&r.record.source) {
            versions.push(r.record.source.clone());
        }
    }
    if !versions.contains(&item.version) {
        versions.push(item.version.clone());
    }
    SourceRecord {
        source: item.version.clone(),
        title: id
            .relative_path()
            .as_str()
            .rsplit('/')
            .next()
            .unwrap_or("source")
            .into(),
        size: item.size,
        media_type: item.media_type,
        extraction_status: crate::extract_bytes(item.media_type, &item.bytes).status,
        present: true,
        captured_at: at.to_owned(),
        versions,
    }
}
fn missing_sources<'a>(
    root: &Path,
    admission: &kb_core::AdmissionDocument,
    config: &EffectiveConfig,
    previous: &'a BTreeMap<kb_core::SourceId, source_record::StoredRecord>,
    current: &BTreeMap<kb_core::SourceId, &crate::DiscoveredSource>,
    skipped: &[crate::SkippedSource],
) -> Result<Vec<(&'a kb_core::SourceId, &'a source_record::StoredRecord)>, KbError> {
    let mut deleted = Vec::new();
    for (id, record) in previous {
        let Some(entry) = admission
            .directories
            .iter()
            .find(|e| e.enabled && e.id == id.admission_id())
        else {
            continue;
        };
        if !record.record.present
            || current.contains_key(id)
            || !crate::discovery::includes_path(entry, config, id.relative_path().as_str())?
        {
            continue;
        }
        let relative = format!("{}/{}", entry.path, id.relative_path().as_str());
        // A skipped file (or a descendant of a skipped directory) is not a deletion.
        if skipped
            .iter()
            .any(|s| relative == s.path || relative.starts_with(&format!("{}/", s.path)))
        {
            continue;
        }
        let path = safe_path(root, &relative)?;
        match fs::symlink_metadata(&path) {
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => deleted.push((id, record)),
            Err(e) => return Err(crate::source_io::io("inspect missing source", &path, e)),
        }
    }

    Ok(deleted)
}
fn persist_plan(
    root: &Path,
    paths: &UserPaths,
    config: &EffectiveConfig,
    inputs: Vec<SourceInput>,
    mut writes: Vec<RecordWrite>,
    at: String,
) -> Result<Option<OperationId>, KbError> {
    let mut budget = Budget::new(config);
    Ok(if inputs.is_empty() {
        None
    } else {
        let id = OperationId::new();
        let log_path = safe_path(root, "Wiki/log.md")?;
        let log_bytes = budget.read(&log_path)?;
        let log = std::str::from_utf8(&log_bytes)
            .map_err(|e| KbError::invalid_config("Wiki/log.md", e.to_string()))?;
        let log_content = append_log(
            log,
            &format!(
                "- {at} · Source capture {id}: {} updated, {} missing.\n",
                inputs.iter().filter(|i| i.present).count(),
                inputs.iter().filter(|i| !i.present).count()
            ),
        )?;
        writes.push(RecordWrite {
            path: PortableRelativePath::parse("Wiki/log.md")?,
            before_sha256: Some(hash(&log_bytes)),
            content: log_content,
        });
        let plan = SourceCapturePlan {
            schema_version: CURRENT_SCHEMA_VERSION,
            operation_id: id,
            kind: CaptureKind::CaptureSources,
            vault_id: config.vault_id,
            target: root.to_path_buf(),
            admission_sha256: hash(&budget.read(&safe_path(root, "admission.yml")?)?),
            config_sha256: read_config_hash(config)?,
            inputs,
            writes,
            created_at: at,
            app_version: env!("CARGO_PKG_VERSION").into(),
        };
        let directory = operation_directory(paths, id);
        private_directory(&directory)?;
        write_json(&directory.join("plan.json"), &plan)?;
        let digest = hash(
            &serde_json::to_vec(&plan)
                .map_err(|e| KbError::invalid_config("plan", e.to_string()))?,
        );
        write_json(&directory.join("plan.sha256"), &digest)?;
        Some(id)
    })
}
fn append_log(text: &str, line: &str) -> Result<String, KbError> {
    let start = "<!-- kb:managed:start -->";
    let end = "<!-- kb:managed:end -->";
    if text.matches(start).count() != 1 || text.matches(end).count() != 1 {
        return Err(KbError::invalid_config(
            "Wiki/log.md",
            "expected one managed region",
        ));
    }
    let begin = text.find(start).expect("count checked");
    let stop = text.find(end).expect("count checked");
    if stop < begin {
        return Err(KbError::invalid_config(
            "Wiki/log.md",
            "reversed managed markers",
        ));
    }
    Ok(format!("{}{}{}", &text[..stop], line, &text[stop..]))
}
pub(crate) fn private_directory(path: &Path) -> Result<(), KbError> {
    fs::create_dir_all(path)
        .map_err(|e| crate::source_io::io("create operation directory", path, e))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .map_err(|e| crate::source_io::io("set private directory permissions", path, e))?;
    }
    Ok(())
}
