use crate::{
    ConfigOverrides, LockMode, UserPaths, VaultLock, load_effective_config,
    operation::{operation_directory, read_json, write_json},
    source_io::{Budget, hash, io, read_config_hash, safe_path},
    source_record,
};
use kb_core::{
    CURRENT_SCHEMA_VERSION, EffectiveConfig, ErrorCode, KbError, KnowledgePlan,
    KnowledgePlanResult, OperationEventKind, OperationId, OperationKind, PortableRelativePath,
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, fs, path::Path};

const MARKER: &str = ".kb/runtime/knowledge-pending.json";
const SOURCE_MARKER: &str = ".kb/runtime/source-pending.json";

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

/// Apply an exact, reviewed knowledge plan and recover interrupted writes.
///
/// # Errors
///
/// Returns stale-plan, integrity, IO, or recovery errors without silently
/// overwriting independently changed files.
pub fn apply_knowledge(
    paths: &UserPaths,
    id: OperationId,
    overrides: &ConfigOverrides,
) -> Result<KnowledgePlanResult, KbError> {
    let result = apply_inner(paths, id, overrides, None);
    if result.is_err() {
        crate::operation_events::record_failed_if_known(paths, id);
    }
    result
}

fn apply_inner(
    paths: &UserPaths,
    id: OperationId,
    overrides: &ConfigOverrides,
    fail_after: Option<usize>,
) -> Result<KnowledgePlanResult, KbError> {
    let directory = operation_directory(paths, id);
    let result_path = directory.join("result.json");
    let plan: KnowledgePlan = read_json(&directory.join("plan.json"))?;
    validate_digest(&directory, &plan)?;
    validate_plan_identity(&plan, id)?;

    let root = &plan.target;
    let _lock = VaultLock::acquire(root, LockMode::Exclusive, "apply knowledge", Some(id))?;
    if safe_path(root, SOURCE_MARKER)?.exists() {
        return Err(recovery(
            "An interrupted source capture needs recovery first.",
        ));
    }
    let marker = safe_path(root, MARKER)?;
    let progress_path = directory.join("knowledge-progress.json");
    if marker.exists() {
        let pending: OperationId = read_json(&marker)?;
        if pending != id {
            return Err(recovery(format!(
                "Another knowledge operation {pending} is pending."
            )));
        }
    }

    if result_path.is_file() {
        let result: KnowledgePlanResult = read_json(&result_path)?;
        if result.operation_id != id
            || result.vault_id != plan.vault_id
            || result.target != plan.target
        {
            return Err(recovery("Knowledge receipt identity mismatch."));
        }
        remove_if_present(&marker)?;
        remove_if_present(&progress_path)?;
        crate::operation_events::record_operation_event_now(
            paths,
            id,
            OperationEventKind::Applied,
            Some((plan.writes.len() as u64, plan.writes.len() as u64)),
            "Knowledge save is complete.",
        )?;
        return finish_housekeeping(root, &result_path, result);
    }

    crate::operation_events::record_operation_event_now(
        paths,
        id,
        OperationEventKind::Applying,
        Some((0, plan.writes.len() as u64)),
        "Knowledge apply started.",
    )?;

    let config = load_effective_config(root, paths, overrides)?;
    if config.schema_version != CURRENT_SCHEMA_VERSION || config.vault_id != plan.vault_id {
        return Err(stale("Vault schema or identity changed."));
    }
    validate_write_set(&plan)?;
    if progress_path.exists() {
        crate::operation_events::record_operation_event_now(
            paths,
            id,
            OperationEventKind::Recovering,
            None,
            "Interrupted knowledge save is being restored.",
        )?;
        let progress: Progress = read_json(&progress_path)?;
        validate_progress(&plan, &progress)?;
        restore(root, &progress, &config)?;
        remove_if_present(&progress_path)?;
        remove_if_present(&marker)?;
    }
    preflight(&plan, &config)?;
    save_writes(paths, root, &config, id, &directory, &plan, fail_after)?;

    let result = KnowledgePlanResult {
        kind: OperationKind::SaveKnowledge,
        operation_id: id,
        vault_id: plan.vault_id,
        target: root.clone(),
        changed: plan.writes.iter().map(|write| write.path.clone()).collect(),
        warnings: Vec::new(),
    };
    // The durable receipt is the commit point. Before it exists, retry restores
    // all operation-owned writes and starts from the reviewed preflight state.
    write_json(&result_path, &result)?;
    crate::operation_events::record_operation_event_now(
        paths,
        id,
        OperationEventKind::Applied,
        Some((plan.writes.len() as u64, plan.writes.len() as u64)),
        "Knowledge save is complete.",
    )?;
    #[cfg(test)]
    crash_for_test("receipt");
    remove_if_present(&marker)?;
    remove_if_present(&progress_path)?;
    finish_housekeeping(root, &result_path, result)
}

fn finish_housekeeping(
    root: &Path,
    result_path: &Path,
    mut result: KnowledgePlanResult,
) -> Result<KnowledgePlanResult, KbError> {
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

fn validate_plan_identity(plan: &KnowledgePlan, id: OperationId) -> Result<(), KbError> {
    if plan.operation_id != id
        || plan.kind != OperationKind::SaveKnowledge
        || plan.schema_version != CURRENT_SCHEMA_VERSION
        || !plan.target.is_absolute()
    {
        return Err(KbError::invalid_config(
            "knowledge plan",
            "identity, kind, version, or target mismatch",
        ));
    }
    Ok(())
}

fn validate_write_set(plan: &KnowledgePlan) -> Result<(), KbError> {
    let expected_changes = plan
        .changes
        .iter()
        .map(|change| PortableRelativePath::parse(&format!("Wiki/{}", change.path.as_str())))
        .collect::<Result<BTreeSet<_>, _>>()?;
    let mut actual = BTreeSet::new();
    for write in &plan.writes {
        if !actual.insert(write.path.clone()) {
            return Err(KbError::invalid_config(
                "knowledge plan",
                "duplicate write target",
            ));
        }
        let path = write.path.as_str();
        if path != "Wiki/index.md"
            && path != "Wiki/log.md"
            && !expected_changes.contains(&write.path)
        {
            return Err(KbError::invalid_config(
                "knowledge plan",
                format!("unexpected write target {path}"),
            ));
        }
    }
    let actual_changes = actual
        .iter()
        .filter(|path| !matches!(path.as_str(), "Wiki/index.md" | "Wiki/log.md"))
        .cloned()
        .collect::<BTreeSet<_>>();
    if actual_changes != expected_changes
        || !actual.contains(&PortableRelativePath::parse("Wiki/index.md")?)
        || !actual.contains(&PortableRelativePath::parse("Wiki/log.md")?)
    {
        return Err(KbError::invalid_config(
            "knowledge plan",
            "write set does not match requested and derived files",
        ));
    }
    Ok(())
}

fn preflight(plan: &KnowledgePlan, config: &EffectiveConfig) -> Result<(), KbError> {
    let created = time::OffsetDateTime::parse(
        &plan.created_at,
        &time::format_description::well_known::Rfc3339,
    )
    .map_err(|error| KbError::invalid_config("plan creation time", error.to_string()))?;
    let age = (time::OffsetDateTime::now_utc() - created).whole_seconds();
    if age < -300
        || i128::from(age) > i128::from(config.operations.plan_retention_hours.value) * 3600
        || plan.app_version != env!("CARGO_PKG_VERSION")
    {
        return Err(stale(
            "Plan expired, has a future timestamp, or uses another application version.",
        ));
    }
    let root = &plan.target;
    let mut budget = Budget::new(config);
    if hash(&budget.read(&safe_path(root, "admission.yml")?)?) != plan.admission_sha256
        || read_config_hash(config)? != plan.config_sha256
    {
        return Err(stale("Admission or read configuration changed."));
    }
    let available = source_record::inventory(root, config, &mut budget)?
        .into_values()
        .flat_map(|stored| stored.record.versions)
        .map(|version| version.exact_uri())
        .collect::<BTreeSet<_>>();
    if plan
        .source_versions
        .iter()
        .any(|source| !available.contains(source))
    {
        return Err(stale(
            "A cited exact source version is no longer available.",
        ));
    }
    for write in &plan.writes {
        let path = safe_path(root, write.path.as_str())?;
        let current = if path.exists() {
            Some(hash(&budget.read(&path)?))
        } else {
            None
        };
        if current != write.before_sha256 {
            return Err(stale(format!(
                "{} changed after planning.",
                write.path.as_str()
            )));
        }
    }
    Ok(())
}

fn save_writes(
    paths: &UserPaths,
    root: &Path,
    config: &EffectiveConfig,
    id: OperationId,
    directory: &Path,
    plan: &KnowledgePlan,
    fail_after: Option<usize>,
) -> Result<(), KbError> {
    let marker = safe_path(root, MARKER)?;
    let progress_path = directory.join("knowledge-progress.json");
    let mut progress = Progress {
        operation_id: id,
        entries: Vec::new(),
    };
    write_json(&progress_path, &progress)?;
    write_json(&marker, &id)?;
    #[cfg(test)]
    crash_for_test("marker");
    let outcome = (|| {
        for write in &plan.writes {
            let destination = safe_path(root, write.path.as_str())?;
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent)
                    .map_err(|error| io("create destination directory", parent, error))?;
            }
            let before = if destination.exists() {
                Some(Budget::new(config).read(&destination)?)
            } else {
                None
            };
            if before.as_ref().map(|bytes| hash(bytes)) != write.before_sha256 {
                return Err(stale(format!(
                    "{} changed before saving.",
                    write.path.as_str()
                )));
            }
            let bytes = write.content.as_bytes();
            progress.entries.push(Effect {
                path: write.path.clone(),
                before: before.clone(),
                after_sha256: hash(bytes),
            });
            write_json(&progress_path, &progress)?;
            if before.is_some() {
                crate::atomic_replace(&destination, bytes)?;
            } else {
                crate::storage::create_new(&destination, bytes)?;
            }
            if crate::operation_events::should_record_progress(
                progress.entries.len() as u64,
                plan.writes.len() as u64,
            ) {
                crate::operation_events::record_operation_event_now(
                    paths,
                    id,
                    OperationEventKind::Progress,
                    Some((progress.entries.len() as u64, plan.writes.len() as u64)),
                    "Knowledge save progress was recorded.",
                )?;
            }
            #[cfg(test)]
            crash_for_test(&format!("write-{}", progress.entries.len()));
            if fail_after == Some(progress.entries.len()) {
                return Err(io("injected interruption", &destination, "test failure"));
            }
        }
        Ok(())
    })();
    if let Err(error) = outcome {
        restore(root, &progress, config)?;
        remove_if_present(&progress_path)?;
        remove_if_present(&marker)?;
        return Err(error);
    }
    Ok(())
}

fn validate_digest(directory: &Path, plan: &KnowledgePlan) -> Result<(), KbError> {
    let expected: String = read_json(&directory.join("plan.sha256"))?;
    let bytes = serde_json::to_vec(plan)
        .map_err(|error| KbError::invalid_config("knowledge plan", error.to_string()))?;
    if hash(&bytes) != expected {
        return Err(stale("Reviewed plan contents changed."));
    }
    Ok(())
}

fn validate_progress(plan: &KnowledgePlan, progress: &Progress) -> Result<(), KbError> {
    if progress.operation_id != plan.operation_id {
        return Err(recovery("Knowledge progress identity mismatch."));
    }
    let mut seen = BTreeSet::new();
    for effect in &progress.entries {
        if !seen.insert(effect.path.clone()) {
            return Err(recovery("Duplicate knowledge recovery path."));
        }
        let valid = plan.writes.iter().any(|write| {
            write.path == effect.path
                && write.before_sha256 == effect.before.as_ref().map(|bytes| hash(bytes))
                && hash(write.content.as_bytes()) == effect.after_sha256
        });
        if !valid {
            return Err(recovery(
                "Knowledge progress is outside its operation plan.",
            ));
        }
    }
    Ok(())
}

fn restore(root: &Path, progress: &Progress, config: &EffectiveConfig) -> Result<(), KbError> {
    for effect in progress.entries.iter().rev() {
        let destination = safe_path(root, effect.path.as_str())?;
        let current = if destination.exists() {
            Some(hash(&Budget::new(config).read(&destination)?))
        } else {
            None
        };
        let before = effect.before.as_ref().map(|bytes| hash(bytes));
        if current == before {
            continue;
        }
        if current.as_deref() != Some(effect.after_sha256.as_str()) {
            return Err(recovery(format!(
                "Preserved independently changed {}.",
                effect.path.as_str()
            )));
        }
        if let Some(bytes) = &effect.before {
            crate::atomic_replace(&destination, bytes)?;
        } else {
            fs::remove_file(&destination)
                .map_err(|error| io("remove operation-owned file", &destination, error))?;
        }
    }
    Ok(())
}

fn remove_if_present(path: &Path) -> Result<(), KbError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(io("remove recovery state", path, error)),
    }
}

fn stale(message: impl Into<String>) -> KbError {
    KbError::new(
        ErrorCode::PlanStale,
        message,
        false,
        "Read the current Vault and create a new knowledge plan.",
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

#[cfg(test)]
fn crash_for_test(point: &str) {
    if std::env::var("KB_KNOWLEDGE_CRASH_POINT").as_deref() == Ok(point) {
        std::process::exit(88);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{InitRequest, create_knowledge_plan, init_vault};
    use kb_core::{
        KnowledgeChangeRequest, KnowledgePlanRequest, OperationEventKind, PortableRelativePath,
    };

    fn paths(base: &Path) -> UserPaths {
        UserPaths::new(base.join("config"), base.join("state"), base.join("cache"))
    }

    fn setup(base: &Path) -> (UserPaths, KnowledgePlan) {
        let root = base.join("vault");
        let user_paths = paths(base);
        init_vault(&InitRequest {
            target: root.clone(),
        })
        .unwrap();
        let config =
            load_effective_config(&root, &user_paths, &ConfigOverrides::default()).unwrap();
        let generated_at = time::OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap();
        let request = KnowledgePlanRequest {
            schema_version: CURRENT_SCHEMA_VERSION,
            changes: vec![KnowledgeChangeRequest {
                path: PortableRelativePath::parse("articles/recovery.md").unwrap(),
                before_sha256: None,
                summary: "Exercise recoverable knowledge saving.".into(),
                content: format!(
                    "---\ntype: Article\ntitle: Recovery\nstatus: stable\ngenerated:\n  by: process:test\n  at: {generated_at}\nsources:\n  - id: source\n    resource: https://example.com\nkb:\n  managed: true\n---\n\n# Recovery\nDurable.\n"
                ),
            }],
        };
        let plan = create_knowledge_plan(
            &root,
            &user_paths,
            &config,
            request,
            time::OffsetDateTime::now_utc(),
        )
        .unwrap();
        (user_paths, plan)
    }

    #[test]
    fn crash_child() {
        let Ok(base) = std::env::var("KB_KNOWLEDGE_CHILD_ROOT") else {
            return;
        };
        let id = std::env::var("KB_KNOWLEDGE_CHILD_ID")
            .unwrap()
            .parse()
            .unwrap();
        apply_knowledge(&paths(Path::new(&base)), id, &ConfigOverrides::default()).unwrap();
        panic!("child did not reach requested crash point");
    }

    fn crash(base: &Path, id: OperationId, point: &str) {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "knowledge_apply::tests::crash_child",
                "--nocapture",
            ])
            .env("KB_KNOWLEDGE_CHILD_ROOT", base)
            .env("KB_KNOWLEDGE_CHILD_ID", id.to_string())
            .env("KB_KNOWLEDGE_CRASH_POINT", point)
            .output()
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(88),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn process_exit_at_each_boundary_is_recoverable() {
        for point in ["marker", "write-1", "write-2", "write-3", "receipt"] {
            let temporary = tempfile::tempdir().unwrap();
            let (user_paths, plan) = setup(temporary.path());
            fs::write(plan.target.join(".kb/cache/catalog.json"), "stale catalog").unwrap();
            fs::write(plan.target.join(".kb/cache/bm25.json"), "stale index").unwrap();
            crash(temporary.path(), plan.operation_id, point);
            assert!(plan.target.join(MARKER).exists());
            let result =
                apply_knowledge(&user_paths, plan.operation_id, &ConfigOverrides::default())
                    .unwrap();
            assert_eq!(result.changed.len(), 3);
            assert!(!plan.target.join(MARKER).exists());
            assert!(!plan.target.join(".kb/cache/catalog.json").exists());
            assert!(!plan.target.join(".kb/cache/bm25.json").exists());
            for write in &plan.writes {
                assert_eq!(
                    fs::read_to_string(plan.target.join(write.path.as_str())).unwrap(),
                    write.content
                );
            }
            let events =
                crate::operation_events::operation_events(&user_paths, plan.operation_id).unwrap();
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
            apply_knowledge(&user_paths, plan.operation_id, &ConfigOverrides::default()).unwrap();
            assert_eq!(
                crate::operation_events::operation_events(&user_paths, plan.operation_id)
                    .unwrap()
                    .events
                    .len(),
                event_count
            );
        }
    }

    #[test]
    fn ordinary_failure_restores_every_write() {
        let temporary = tempfile::tempdir().unwrap();
        let (user_paths, plan) = setup(temporary.path());
        let before_index = fs::read(plan.target.join("Wiki/index.md")).unwrap();
        let before_log = fs::read(plan.target.join("Wiki/log.md")).unwrap();

        let error = apply_inner(
            &user_paths,
            plan.operation_id,
            &ConfigOverrides::default(),
            Some(2),
        )
        .unwrap_err();

        assert_eq!(error.code, ErrorCode::IoFailure);
        assert!(!plan.target.join("Wiki/articles/recovery.md").exists());
        assert_eq!(
            fs::read(plan.target.join("Wiki/index.md")).unwrap(),
            before_index
        );
        assert_eq!(
            fs::read(plan.target.join("Wiki/log.md")).unwrap(),
            before_log
        );
        assert!(!plan.target.join(MARKER).exists());
    }

    #[test]
    fn recovery_preserves_independent_edits_then_resumes() {
        let temporary = tempfile::tempdir().unwrap();
        let (user_paths, plan) = setup(temporary.path());
        crash(temporary.path(), plan.operation_id, "write-2");
        let article = plan
            .writes
            .iter()
            .find(|write| write.path.as_str() == "Wiki/articles/recovery.md")
            .unwrap();
        let destination = plan.target.join(article.path.as_str());
        fs::write(&destination, "independent edit").unwrap();

        let error = apply_knowledge(&user_paths, plan.operation_id, &ConfigOverrides::default())
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::VaultNeedsRecovery);
        assert_eq!(
            fs::read_to_string(&destination).unwrap(),
            "independent edit"
        );
        assert!(plan.target.join(MARKER).exists());

        fs::write(&destination, &article.content).unwrap();
        apply_knowledge(&user_paths, plan.operation_id, &ConfigOverrides::default()).unwrap();
        assert!(!plan.target.join(MARKER).exists());
    }
}
