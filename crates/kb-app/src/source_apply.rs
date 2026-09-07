use crate::{
    ConfigOverrides, LockMode, UserPaths, VaultLock, load_admission, load_effective_config,
    operation::{operation_directory, read_json, write_json},
    source_io::{Budget, hash, io, read_config_hash, safe_path},
    source_plan::{CaptureKind, SourceCapturePlan, SourceCaptureResult},
    source_record,
};
use kb_core::{
    CURRENT_SCHEMA_VERSION, EffectiveConfig, ErrorCode, KbError, OperationEventKind, OperationId,
    PortableRelativePath,
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, fs, path::Path};
const MARKER: &str = ".kb/runtime/source-pending.json";
const KNOWLEDGE_MARKER: &str = ".kb/runtime/knowledge-pending.json";
#[derive(Debug, Serialize, Deserialize)]
struct Progress {
    operation_id: OperationId,
    entries: Vec<Effect>,
}
#[derive(Debug, Serialize, Deserialize)]
struct Effect {
    path: PortableRelativePath,
    before: Option<Vec<u8>>,
    after_sha256: String,
}
/// Apply an exact reviewed source plan, restoring partial writes on failure.
/// # Errors
/// Returns stale-plan, IO, integrity or recovery errors.
pub(crate) fn apply_capture(
    paths: &UserPaths,
    id: OperationId,
    overrides: &ConfigOverrides,
) -> Result<SourceCaptureResult, KbError> {
    let result = apply_inner(paths, id, overrides, None);
    if result.is_err() {
        crate::operation_events::record_failed_if_known(paths, id);
    }
    result
}
fn stale(message: impl Into<String>) -> KbError {
    KbError::new(
        ErrorCode::PlanStale,
        message,
        false,
        "Run kb review and inspect a fresh plan.",
    )
}
fn recovery(message: impl Into<String>) -> KbError {
    KbError::new(
        ErrorCode::VaultNeedsRecovery,
        message,
        false,
        "Retry the recorded kb apply operation; preserve files changed by other writers.",
    )
}
pub(crate) fn ensure_no_pending(root: &Path) -> Result<(), KbError> {
    if safe_path(root, MARKER)?.exists() {
        return Err(recovery("An interrupted source capture needs recovery."));
    }
    if safe_path(root, KNOWLEDGE_MARKER)?.exists() {
        return Err(recovery("An interrupted knowledge save needs recovery."));
    }
    Ok(())
}
fn apply_inner(
    paths: &UserPaths,
    id: OperationId,
    overrides: &ConfigOverrides,
    fail_after: Option<usize>,
) -> Result<SourceCaptureResult, KbError> {
    let directory = operation_directory(paths, id);
    let result_path = directory.join("result.json");
    let plan: SourceCapturePlan = read_json(&directory.join("plan.json"))?;
    validate_digest(&directory, &plan)?;
    if plan.operation_id != id
        || plan.schema_version != CURRENT_SCHEMA_VERSION
        || !plan.target.is_absolute()
    {
        return Err(KbError::invalid_config(
            "source plan",
            "identity or version mismatch",
        ));
    }
    let root = &plan.target;
    safe_path(root, ".kb/runtime")?;
    let _lock = VaultLock::acquire(root, LockMode::Exclusive, "apply source capture", Some(id))?;
    let marker = safe_path(root, MARKER)?;
    if safe_path(root, KNOWLEDGE_MARKER)?.exists() {
        return Err(recovery(
            "An interrupted knowledge save needs recovery first.",
        ));
    }
    let progress_path = directory.join("source-progress.json");
    if marker.exists() {
        let pending: OperationId = read_json(&marker)?;
        if pending != id {
            return Err(recovery(format!(
                "Another source operation {pending} is pending."
            )));
        }
    }
    if result_path.is_file() {
        let result: SourceCaptureResult = read_json(&result_path)?;
        if result.operation_id != id
            || result.vault_id != plan.vault_id
            || result.target != plan.target
        {
            return Err(recovery("Receipt identity mismatch."));
        }
        // A durable receipt means knowledge was saved; only housekeeping remains.
        remove_if_present(&marker)?;
        remove_if_present(&progress_path)?;
        record_source_complete(paths, id, plan.writes.len())?;
        return finish_housekeeping(root, &result_path, result);
    }
    record_source_start(paths, id, plan.writes.len())?;
    let config = load_effective_config(root, paths, overrides)?;
    if config.schema_version != CURRENT_SCHEMA_VERSION || config.vault_id != plan.vault_id {
        return Err(stale("Vault schema or identity changed."));
    }
    validate_plan(&plan)?;
    if progress_path.exists() {
        record_source_recovery(paths, id)?;
        let progress: Progress = read_json(&progress_path)?;
        validate_progress(&plan, &progress)?;
        restore(root, &progress, &config)?;
        fs::remove_file(&progress_path).map_err(|e| io("remove progress", &progress_path, e))?;
        if marker.exists() {
            fs::remove_file(&marker).map_err(|e| io("remove recovery marker", &marker, e))?;
        }
    }
    preflight(&plan, paths, overrides, &config)?;
    let outputs = prepare_outputs(&plan, &config)?;
    save_outputs(paths, root, &config, id, &directory, outputs, fail_after)?;
    let result = SourceCaptureResult {
        kind: CaptureKind::CaptureSources,
        operation_id: id,
        vault_id: plan.vault_id,
        target: root.clone(),
        captured: plan
            .inputs
            .iter()
            .filter(|i| i.present)
            .map(|i| i.version.exact_uri())
            .collect(),
        marked_missing: plan
            .inputs
            .iter()
            .filter(|i| !i.present)
            .map(|i| i.version.source.logical_uri())
            .collect(),
        warnings: cache_extractions(&plan, &config),
    };
    // The receipt is the commit point. Until it exists, recovery restores the old knowledge.
    write_json(&result_path, &result)?;
    record_source_complete(paths, id, plan.writes.len())?;
    #[cfg(test)]
    crash_for_test("receipt");
    fs::remove_file(&marker).map_err(|e| io("remove marker", &marker, e))?;
    fs::remove_file(&progress_path).map_err(|e| io("remove progress", &progress_path, e))?;
    finish_housekeeping(root, &result_path, result)
}

fn finish_housekeeping(
    root: &Path,
    result_path: &Path,
    mut result: SourceCaptureResult,
) -> Result<SourceCaptureResult, KbError> {
    let warnings = crate::search::invalidate_caches(root);
    let changed = warnings
        .iter()
        .any(|warning| !result.warnings.contains(warning));
    for warning in warnings {
        if !result.warnings.contains(&warning) {
            result.warnings.push(warning);
        }
    }
    if changed {
        write_json(result_path, &result)?;
    }
    Ok(result)
}
#[cfg(test)]
fn crash_for_test(point: &str) {
    if std::env::var("KB_CAPTURE_CRASH_POINT").as_deref() == Ok(point) {
        std::process::exit(87);
    }
}

struct Output {
    path: PortableRelativePath,
    before: Option<Vec<u8>>,
    bytes: Vec<u8>,
}
fn prepare_outputs(
    plan: &SourceCapturePlan,
    config: &EffectiveConfig,
) -> Result<Vec<Output>, KbError> {
    let root = &plan.target;
    let mut budget = Budget::new(config);
    let mut outputs = Vec::new();
    let mut object_seen = BTreeSet::new();
    for input in plan.inputs.iter().filter(|i| i.present) {
        let p = safe_path(root, input.relative_path.as_str())?;
        let bytes = budget.read(&p)?;
        if hash(&bytes) != input.version.sha256() {
            return Err(stale("Source changed during preflight."));
        }
        let object = input.version.object_path();
        if !object_seen.insert(object.clone()) {
            continue;
        }
        let destination = safe_path(root, &object)?;
        if destination.exists() {
            if hash(&budget.read(&destination)?) != input.version.sha256() {
                return Err(integrity(&object));
            }
        } else {
            outputs.push(Output {
                path: PortableRelativePath::parse(&object)?,
                before: None,
                bytes,
            });
        }
    }
    for write in &plan.writes {
        if write.content.len() as u64 > config.limits.max_file_bytes.value {
            return Err(crate::source_io::limit(
                "limits.max_file_bytes",
                config.limits.max_file_bytes.value,
                &root.join(write.path.as_str()),
            ));
        }
        let p = safe_path(root, write.path.as_str())?;
        let before = if p.exists() {
            Some(budget.read(&p)?)
        } else {
            None
        };
        if before.as_ref().map(|v| hash(v)) != write.before_sha256 {
            return Err(stale(format!("{} changed", write.path.as_str())));
        }
        outputs.push(Output {
            path: write.path.clone(),
            before,
            bytes: write.content.as_bytes().to_vec(),
        });
    }
    Ok(outputs)
}
fn save_outputs(
    paths: &UserPaths,
    root: &Path,
    config: &EffectiveConfig,
    id: OperationId,
    directory: &Path,
    outputs: Vec<Output>,
    fail_after: Option<usize>,
) -> Result<(), KbError> {
    let total = outputs.len() as u64;
    let marker = safe_path(root, MARKER)?;
    let progress_path = directory.join("source-progress.json");
    let mut progress = Progress {
        operation_id: id,
        entries: Vec::new(),
    };
    // Publish a recoverable empty journal before exposing the pending marker.
    write_json(&progress_path, &progress)?;
    write_json(&marker, &id)?;
    #[cfg(test)]
    crash_for_test("marker");
    let outcome = (|| {
        for Output {
            path,
            before,
            bytes,
        } in outputs
        {
            let dest = safe_path(root, path.as_str())?;
            if let Some(parent) = dest.parent() {
                fs::create_dir_all(parent)
                    .map_err(|e| io("create destination directory", parent, e))?;
            }
            let now = if dest.exists() {
                Some(Budget::new(config).read(&dest)?)
            } else {
                None
            };
            if now != before {
                return Err(stale(format!("{} changed before saving", path.as_str())));
            }
            let is_new = before.is_none();
            progress.entries.push(Effect {
                path,
                before,
                after_sha256: hash(&bytes),
            });
            write_json(&progress_path, &progress)?;
            if is_new {
                crate::storage::create_new(&dest, &bytes)?;
            } else {
                crate::atomic_replace(&dest, &bytes)?;
            }
            record_source_progress(paths, id, progress.entries.len() as u64, total)?;
            #[cfg(test)]
            crash_for_test(&format!("write-{}", progress.entries.len()));
            if fail_after == Some(progress.entries.len()) {
                return Err(io("injected interruption", &dest, "test failure"));
            }
        }
        for entry in &progress.entries {
            let p = safe_path(root, entry.path.as_str())?;
            if hash(&Budget::new(config).read(&p)?) != entry.after_sha256 {
                return Err(recovery(format!(
                    "{} changed during apply",
                    entry.path.as_str()
                )));
            }
        }
        Ok(())
    })();
    if let Err(e) = outcome {
        restore(root, &progress, config)?;
        if progress_path.exists() {
            fs::remove_file(&progress_path)
                .map_err(|err| io("remove progress", &progress_path, err))?;
        }
        fs::remove_file(&marker).map_err(|err| io("remove marker", &marker, err))?;
        return Err(e);
    }
    Ok(())
}

fn record_source_start(paths: &UserPaths, id: OperationId, total: usize) -> Result<(), KbError> {
    crate::operation_events::record_operation_event_now(
        paths,
        id,
        OperationEventKind::Applying,
        Some((0, total as u64)),
        "Source capture apply started.",
    )?;
    Ok(())
}

fn record_source_recovery(paths: &UserPaths, id: OperationId) -> Result<(), KbError> {
    crate::operation_events::record_operation_event_now(
        paths,
        id,
        OperationEventKind::Recovering,
        None,
        "Interrupted source capture is being restored.",
    )?;
    Ok(())
}

fn record_source_progress(
    paths: &UserPaths,
    id: OperationId,
    completed: u64,
    total: u64,
) -> Result<(), KbError> {
    crate::operation_events::record_operation_event_now(
        paths,
        id,
        OperationEventKind::Progress,
        Some((completed, total)),
        "Source capture progress was saved.",
    )?;
    Ok(())
}

fn record_source_complete(paths: &UserPaths, id: OperationId, total: usize) -> Result<(), KbError> {
    crate::operation_events::record_operation_event_now(
        paths,
        id,
        OperationEventKind::Applied,
        Some((total as u64, total as u64)),
        "Source capture is complete.",
    )?;
    Ok(())
}

fn remove_if_present(path: &Path) -> Result<(), KbError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(io("remove recovery state", path, e)),
    }
}
fn cache_extractions(plan: &SourceCapturePlan, config: &EffectiveConfig) -> Vec<String> {
    let mut warnings = Vec::new();
    let mut budget = Budget::new(config);
    for input in plan.inputs.iter().filter(|i| i.present) {
        let outcome = (|| {
            let bytes = budget.read(&safe_path(&plan.target, &input.version.object_path())?)?;
            let extracted = crate::extract_bytes(input.media_type, &bytes);
            let dir = safe_path(
                &plan.target,
                &format!(".kb/cache/extracted/{}", input.version.sha256()),
            )?;
            fs::create_dir_all(&dir).map_err(|e| io("create extraction cache", &dir, e))?;
            let path = safe_path(
                &plan.target,
                &format!(
                    ".kb/cache/extracted/{}/{}-{}-{}.json",
                    input.version.sha256(),
                    extracted.extractor_id,
                    extracted.extractor_version,
                    input.media_type.as_str()
                ),
            )?;
            write_json(&path, &extracted)
        })();
        if let Err(error) = outcome {
            warnings.push(format!(
                "Source saved; extraction cache unavailable: {}",
                error.message
            ));
        }
    }
    warnings
}
pub(crate) fn integrity(path: &str) -> KbError {
    KbError::new(
        ErrorCode::SourceIntegrityFailed,
        format!("Source object hash mismatch: {path}"),
        false,
        "Restore the original object from a verified backup.",
    )
}
fn validate_plan(plan: &SourceCapturePlan) -> Result<(), KbError> {
    let mut expected = plan
        .inputs
        .iter()
        .map(|i| source_record::record_path_for(&i.version.source))
        .collect::<Result<BTreeSet<_>, _>>()?;
    if expected.len() != plan.inputs.len() {
        return Err(KbError::invalid_config("source plan", "duplicate sources"));
    }
    expected.insert(PortableRelativePath::parse("Wiki/log.md")?);
    let actual = plan
        .writes
        .iter()
        .map(|w| w.path.clone())
        .collect::<BTreeSet<_>>();
    if expected != actual || actual.len() != plan.writes.len() {
        return Err(KbError::invalid_config(
            "source plan",
            "unexpected write targets",
        ));
    }
    for input in &plan.inputs {
        let write = plan
            .writes
            .iter()
            .find(|w| {
                w.path
                    == source_record::record_path_for(&input.version.source)
                        .expect("valid identity")
            })
            .expect("set verified");
        let record = source_record::parse(&write.content)?;
        if record.source != input.version
            || record.present != input.present
            || record
                .versions
                .iter()
                .any(|v| v.source != input.version.source)
        {
            return Err(KbError::invalid_config(
                "source plan",
                "record differs from reviewed source",
            ));
        }
    }
    Ok(())
}
fn preflight(
    plan: &SourceCapturePlan,
    _paths: &UserPaths,
    _overrides: &ConfigOverrides,
    config: &EffectiveConfig,
) -> Result<(), KbError> {
    let created = time::OffsetDateTime::parse(
        &plan.created_at,
        &time::format_description::well_known::Rfc3339,
    )
    .map_err(|e| KbError::invalid_config("plan creation time", e.to_string()))?;
    let age = (time::OffsetDateTime::now_utc() - created).whole_seconds();
    if age < -300
        || i128::from(age) > i128::from(config.operations.plan_retention_hours.value) * 3600
        || plan.app_version != env!("CARGO_PKG_VERSION")
    {
        return Err(stale(
            "Plan expired, has a future timestamp, or was created by another application version.",
        ));
    }
    let root = &plan.target;
    let mut b = Budget::new(config);
    if hash(&b.read(&safe_path(root, "admission.yml")?)?) != plan.admission_sha256
        || read_config_hash(config)? != plan.config_sha256
    {
        return Err(stale("Admission or read configuration changed."));
    }
    let admission = load_admission(root)?;
    for input in &plan.inputs {
        let entry = admission
            .directories
            .iter()
            .find(|e| e.enabled && e.id == input.version.source.admission_id())
            .ok_or_else(|| stale("Source is no longer admitted."))?;
        let expected = format!(
            "{}/{}",
            entry.path,
            input.version.source.relative_path().as_str()
        );
        if expected != input.relative_path.as_str() {
            return Err(KbError::invalid_config(
                "source plan",
                "input escapes its admission",
            ));
        }
        let p = safe_path(root, &expected)?;
        if input.present {
            if !p.exists() || hash(&b.read(&p)?) != input.version.sha256() {
                return Err(stale(format!("{expected} changed after review")));
            }
        } else if p.exists() {
            return Err(stale(format!("{expected} exists again")));
        }
    }
    for w in &plan.writes {
        let p = safe_path(root, w.path.as_str())?;
        let digest = if p.exists() {
            Some(hash(&b.read(&p)?))
        } else {
            None
        };
        if digest != w.before_sha256 {
            return Err(stale(format!("{} changed after review", w.path.as_str())));
        }
    }
    Ok(())
}
fn validate_digest(directory: &Path, plan: &SourceCapturePlan) -> Result<(), KbError> {
    let expected: String = read_json(&directory.join("plan.sha256"))?;
    let bytes =
        serde_json::to_vec(plan).map_err(|e| KbError::invalid_config("plan", e.to_string()))?;
    if hash(&bytes) != expected {
        return Err(stale("Reviewed plan contents changed."));
    }
    Ok(())
}
fn validate_progress(plan: &SourceCapturePlan, p: &Progress) -> Result<(), KbError> {
    if p.operation_id != plan.operation_id {
        return Err(recovery("Progress identity mismatch."));
    }
    let mut seen = BTreeSet::new();
    for e in &p.entries {
        if !seen.insert(e.path.clone()) {
            return Err(recovery("Duplicate recovery path."));
        }
        let record = plan.writes.iter().any(|w| {
            w.path == e.path
                && hash(w.content.as_bytes()) == e.after_sha256
                && w.before_sha256 == e.before.as_ref().map(|v| hash(v))
        });
        let object = e.before.is_none()
            && plan.inputs.iter().any(|i| {
                i.present
                    && i.version.object_path() == e.path.as_str()
                    && i.version.sha256() == e.after_sha256
            });
        if !record && !object {
            return Err(recovery("Progress is outside its operation plan."));
        }
    }
    Ok(())
}
fn restore(root: &Path, p: &Progress, c: &EffectiveConfig) -> Result<(), KbError> {
    for e in p.entries.iter().rev() {
        let dest = safe_path(root, e.path.as_str())?;
        let current = if dest.exists() {
            Some(hash(&Budget::new(c).read(&dest)?))
        } else {
            None
        };
        let before = e.before.as_ref().map(|v| hash(v));
        if current == before {
            continue;
        }
        if current.as_deref() != Some(e.after_sha256.as_str()) {
            return Err(recovery(format!(
                "Preserved independently changed {}",
                e.path.as_str()
            )));
        }
        if let Some(bytes) = &e.before {
            crate::atomic_replace(&dest, bytes)?;
        } else {
            fs::remove_file(&dest).map_err(|err| io("remove operation-owned file", &dest, err))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{InitRequest, init_vault, review_sources};
    use kb_core::OperationEventKind;
    fn paths(base: &Path) -> UserPaths {
        UserPaths::new(base.join("config"), base.join("state"), base.join("cache"))
    }
    fn setup(base: &Path) -> (UserPaths, SourceCapturePlan) {
        let root = base.join("vault");
        let user = paths(base);
        init_vault(&InitRequest {
            target: root.clone(),
        })
        .unwrap();
        fs::create_dir(root.join("Notes")).unwrap();
        fs::write(
            root.join("admission.yml"),
            "schema_version: v1.0\ndirectories:\n- id: notes\n  path: Notes\n  enabled: true\n",
        )
        .unwrap();
        fs::write(root.join("Notes/a.md"), "# Source\nneedle\n").unwrap();
        let c = load_effective_config(&root, &user, &ConfigOverrides::default()).unwrap();
        let id = review_sources(&root, &user, &c)
            .unwrap()
            .operation_id
            .unwrap();
        let plan = read_json(&operation_directory(&user, id).join("plan.json")).unwrap();
        (user, plan)
    }
    #[test]
    fn crash_child() {
        let Ok(base) = std::env::var("KB_CAPTURE_CHILD_ROOT") else {
            return;
        };
        let id = std::env::var("KB_CAPTURE_CHILD_ID")
            .unwrap()
            .parse()
            .unwrap();
        apply_capture(&paths(Path::new(&base)), id, &ConfigOverrides::default()).unwrap();
        panic!("child did not reach requested crash point");
    }
    fn crash(base: &Path, id: OperationId, point: &str) {
        let o = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "source_apply::tests::crash_child", "--nocapture"])
            .env("KB_CAPTURE_CHILD_ROOT", base)
            .env("KB_CAPTURE_CHILD_ID", id.to_string())
            .env("KB_CAPTURE_CRASH_POINT", point)
            .output()
            .unwrap();
        assert_eq!(
            o.status.code(),
            Some(87),
            "{}",
            String::from_utf8_lossy(&o.stderr)
        );
    }
    #[test]
    fn process_exit_at_each_write_and_receipt_is_recoverable() {
        for point in ["write-1", "write-2", "write-3", "receipt"] {
            let t = tempfile::tempdir().unwrap();
            let (user, plan) = setup(t.path());
            fs::write(plan.target.join(".kb/cache/catalog.json"), "stale catalog").unwrap();
            fs::write(plan.target.join(".kb/cache/bm25.json"), "stale index").unwrap();
            crash(t.path(), plan.operation_id, point);
            assert!(ensure_no_pending(&plan.target).is_err());
            apply_capture(&user, plan.operation_id, &ConfigOverrides::default()).unwrap();
            assert!(!plan.target.join(MARKER).exists());
            assert!(!plan.target.join(".kb/cache/catalog.json").exists());
            assert!(!plan.target.join(".kb/cache/bm25.json").exists());
            for write in &plan.writes {
                assert_eq!(
                    fs::read_to_string(plan.target.join(write.path.as_str())).unwrap(),
                    write.content
                );
            }
            let bytes = fs::read(plan.target.join(plan.inputs[0].version.object_path())).unwrap();
            assert_eq!(bytes, fs::read(plan.target.join("Notes/a.md")).unwrap());
            let c =
                load_effective_config(&plan.target, &user, &ConfigOverrides::default()).unwrap();
            assert!(
                review_sources(&plan.target, &user, &c)
                    .unwrap()
                    .operation_id
                    .is_none()
            );
            let events =
                crate::operation_events::operation_events(&user, plan.operation_id).unwrap();
            assert_eq!(
                events.events.first().unwrap().kind,
                OperationEventKind::Planned
            );
            assert_eq!(
                events
                    .events
                    .iter()
                    .any(|event| event.kind == OperationEventKind::Recovering),
                point != "receipt"
            );
            assert_eq!(
                events.events.last().unwrap().kind,
                OperationEventKind::Applied
            );
            let event_count = events.events.len();
            apply_capture(&user, plan.operation_id, &ConfigOverrides::default()).unwrap();
            assert_eq!(
                crate::operation_events::operation_events(&user, plan.operation_id)
                    .unwrap()
                    .events
                    .len(),
                event_count
            );
        }
    }
    #[test]
    fn knowledge_pending_blocks_source_apply() {
        let temporary = tempfile::tempdir().unwrap();
        let (user, plan) = setup(temporary.path());
        write_json(&plan.target.join(KNOWLEDGE_MARKER), &OperationId::new()).unwrap();

        let error =
            apply_capture(&user, plan.operation_id, &ConfigOverrides::default()).unwrap_err();

        assert_eq!(error.code, ErrorCode::VaultNeedsRecovery);
        assert!(!plan.target.join(MARKER).exists());
    }
    #[test]
    fn recovery_preserves_independent_edits_and_can_resume_after_restoration() {
        let t = tempfile::tempdir().unwrap();
        let (user, plan) = setup(t.path());
        crash(t.path(), plan.operation_id, "write-2");
        let record = &plan.writes[0];
        let dest = plan.target.join(record.path.as_str());
        fs::write(&dest, "human changes during interruption").unwrap();
        let error =
            apply_capture(&user, plan.operation_id, &ConfigOverrides::default()).unwrap_err();
        assert_eq!(error.code, ErrorCode::VaultNeedsRecovery);
        assert_eq!(
            fs::read_to_string(&dest).unwrap(),
            "human changes during interruption"
        );
        assert!(plan.target.join(MARKER).exists());
        fs::write(&dest, &record.content).unwrap();
        apply_capture(&user, plan.operation_id, &ConfigOverrides::default()).unwrap();
        assert!(!plan.target.join(MARKER).exists());
    }
    #[test]
    fn failed_save_restores_original_log_and_removes_new_files() {
        for count in 1..=3 {
            let t = tempfile::tempdir().unwrap();
            let (user, plan) = setup(t.path());
            let log = fs::read(plan.target.join("Wiki/log.md")).unwrap();
            assert!(
                apply_inner(
                    &user,
                    plan.operation_id,
                    &ConfigOverrides::default(),
                    Some(count)
                )
                .is_err()
            );
            assert_eq!(fs::read(plan.target.join("Wiki/log.md")).unwrap(), log);
            assert!(
                !plan
                    .target
                    .join(plan.inputs[0].version.object_path())
                    .exists()
            );
            assert!(!plan.target.join(plan.writes[0].path.as_str()).exists());
            assert!(!plan.target.join(MARKER).exists());
        }
    }
    #[test]
    fn expired_plan_cannot_write_knowledge() {
        let t = tempfile::tempdir().unwrap();
        let (user, mut plan) = setup(t.path());
        plan.created_at = "2000-01-01T00:00:00Z".into();
        let directory = operation_directory(&user, plan.operation_id);
        write_json(&directory.join("plan.json"), &plan).unwrap();
        write_json(
            &directory.join("plan.sha256"),
            &hash(&serde_json::to_vec(&plan).unwrap()),
        )
        .unwrap();
        let error =
            apply_capture(&user, plan.operation_id, &ConfigOverrides::default()).unwrap_err();
        assert_eq!(error.code, ErrorCode::PlanStale);
        assert!(
            !plan
                .target
                .join(plan.inputs[0].version.object_path())
                .exists()
        );
    }
    #[test]
    fn interruption_before_first_file_does_not_strand_a_stale_plan() {
        let t = tempfile::tempdir().unwrap();
        let (user, plan) = setup(t.path());
        crash(t.path(), plan.operation_id, "marker");
        fs::write(plan.target.join("Notes/a.md"), "new source bytes").unwrap();
        let error =
            apply_capture(&user, plan.operation_id, &ConfigOverrides::default()).unwrap_err();
        assert_eq!(error.code, ErrorCode::PlanStale);
        assert!(!plan.target.join(MARKER).exists());
        let config =
            load_effective_config(&plan.target, &user, &ConfigOverrides::default()).unwrap();
        assert!(
            review_sources(&plan.target, &user, &config)
                .unwrap()
                .operation_id
                .is_some()
        );
    }
}
