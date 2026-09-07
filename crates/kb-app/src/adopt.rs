use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    fs::File,
    io::Read,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use kb_core::{
    AdoptionPlan, CURRENT_SCHEMA_VERSION, ErrorCode, KbError, ObservedEntry, ObservedKind,
    OperationEventKind, OperationId, OperationKind, PlannedFile, PortableRelativePath,
    portability_key,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{
    AdoptionResult, OperationState, UserPaths, atomic_replace, inspect_operation,
    operation::{operation_directory, read_json, save_plan, save_result, write_json},
    register_vault,
    template::{EMPTY_DIRECTORIES, STATIC_FILES, config_yaml},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ApplyProgress {
    operation_id: OperationId,
    stage: PathBuf,
    installed_files: Vec<PortableRelativePath>,
    created_directories: Vec<PortableRelativePath>,
}

struct InstallState {
    installed_files: Vec<PathBuf>,
    created_directories: Vec<PathBuf>,
    progress_path: PathBuf,
    progress: ApplyProgress,
    fail_after_installed_file: Option<usize>,
}

enum AdoptionOperation {
    Planned(AdoptionPlan),
    Applied(AdoptionResult),
}

/// Review an existing directory and persist a read-only adoption plan.
///
/// # Errors
///
/// Returns [`KbError`] for an unsafe target, a framework-owned path collision,
/// an unreadable entry, a non-portable path, or a plan persistence failure.
pub fn create_adoption_plan(
    target: &Path,
    user_paths: &UserPaths,
) -> Result<AdoptionPlan, KbError> {
    let target = absolute_path(target)?;
    kb_core::ensure_not_link_or_reparse_point(&target)?;
    if !target.is_dir() {
        return Err(KbError::new(
            ErrorCode::VaultNotFound,
            format!("Adoption target is not a directory: {}", target.display()),
            false,
            "Choose an existing directory.",
        ));
    }
    reject_owned_collisions(&target)?;
    let observed_entries = snapshot(&target)?;
    let operation_id = OperationId::new();
    let vault_id = Uuid::new_v4();
    let generated = generated_files(vault_id)?;
    let creates = generated
        .iter()
        .map(|(relative_path, bytes)| PlannedFile {
            relative_path: relative_path.clone(),
            size: bytes.len() as u64,
            sha256: hash_bytes(bytes),
        })
        .collect();
    let created_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| KbError::invalid_config("system clock", error.to_string()))?
        .as_secs()
        .to_string();
    let plan = AdoptionPlan {
        schema_version: CURRENT_SCHEMA_VERSION,
        operation_id,
        kind: OperationKind::AdoptVault,
        vault_id,
        target,
        observed_entries,
        creates,
        created_at,
        app_version: env!("CARGO_PKG_VERSION").to_owned(),
    };
    save_plan(user_paths, &plan)?;
    Ok(plan)
}

/// Apply a stored adoption plan after proving its reviewed input is unchanged.
///
/// # Errors
///
/// Returns [`KbError`] when the operation is missing, stale, unsafe to recover,
/// or cannot install and verify every generated path.
pub fn apply_operation(
    user_paths: &UserPaths,
    operation_id: OperationId,
) -> Result<AdoptionResult, KbError> {
    let result = apply_operation_inner(user_paths, operation_id, None);
    if result.is_err() {
        crate::operation_events::record_failed_if_known(user_paths, operation_id);
    }
    result
}

fn apply_operation_inner(
    user_paths: &UserPaths,
    operation_id: OperationId,
    fail_after_installed_file: Option<usize>,
) -> Result<AdoptionResult, KbError> {
    let _lock = operation_lock(user_paths)?;
    let plan = match load_adoption_operation(user_paths, operation_id)? {
        AdoptionOperation::Applied(result) => {
            record_adoption_complete(user_paths, operation_id)?;
            return Ok(result);
        }
        AdoptionOperation::Planned(plan) => plan,
    };
    validate_plan_identity(&plan, operation_id)?;
    record_adoption_start(user_paths, &plan)?;

    let receipt_path = vault_receipt_path(&plan);
    if receipt_path.is_file() {
        let result: AdoptionResult = read_json(&receipt_path)?;
        verify_result(&plan, &result)?;
        save_result(user_paths, &result)?;
        record_adoption_complete(user_paths, operation_id)?;
        return Ok(result);
    }

    let generated = generated_files(plan.vault_id)?;
    verify_planned_files(&plan, &generated)?;
    let progress_path = progress_path(user_paths, operation_id);
    if progress_path.is_file() {
        record_adoption_recovery(user_paths, operation_id)?;
        let progress: ApplyProgress = read_json(&progress_path)?;
        validate_progress(&plan, &progress, &generated)?;
        if generated_is_complete(&plan.target, &generated)? {
            verify_observed_entries(&plan.target, &plan.observed_entries)?;
            return finish_adoption(user_paths, &plan);
        }
        recover_interrupted_apply(&plan, &generated, &progress)?;
        remove_progress(&progress_path)?;
    }

    let current = snapshot(&plan.target)?;
    if current != plan.observed_entries {
        return Err(KbError::new(
            ErrorCode::PlanStale,
            format!(
                "Adoption target changed after operation {} was reviewed.",
                plan.operation_id
            ),
            false,
            format!("Run kb adopt {} again.", plan.target.display()),
        ));
    }

    let stage = plan.target.join(format!(".kb-adopt-{}", plan.operation_id));
    let progress = ApplyProgress {
        operation_id,
        stage: stage.clone(),
        installed_files: Vec::new(),
        created_directories: Vec::new(),
    };
    write_json(&progress_path, &progress)?;
    create_private_directory(&stage)?;
    if let Err(error) = populate_stage(&stage, &generated) {
        let _ = remove_verified_stage(&stage, &generated);
        return Err(error);
    }

    let mut install_state = InstallState {
        installed_files: Vec::new(),
        created_directories: Vec::new(),
        progress_path,
        progress,
        fail_after_installed_file,
    };
    let install = install_generated(
        user_paths,
        operation_id,
        &plan.target,
        &stage,
        &generated,
        &mut install_state,
    );
    if let Err(error) = install {
        return rollback_or_recovery(
            &plan.target,
            &stage,
            &generated,
            &install_state.installed_files,
            &install_state.created_directories,
            error,
        );
    }
    if let Err(error) = remove_verified_stage(&stage, &generated) {
        return rollback_or_recovery(
            &plan.target,
            &stage,
            &generated,
            &install_state.installed_files,
            &install_state.created_directories,
            error,
        );
    }
    if !generated_is_complete(&plan.target, &generated)? {
        return Err(recovery_error(
            &plan.target,
            "generated files did not pass final verification",
        ));
    }
    verify_observed_entries(&plan.target, &plan.observed_entries)?;
    finish_adoption(user_paths, &plan)
}

fn load_adoption_operation(
    user_paths: &UserPaths,
    operation_id: OperationId,
) -> Result<AdoptionOperation, KbError> {
    match inspect_operation(user_paths, operation_id)? {
        OperationState::Applied(result) => Ok(AdoptionOperation::Applied(result)),
        OperationState::Planned(plan) => Ok(AdoptionOperation::Planned(plan)),
        OperationState::PlannedSource(_)
        | OperationState::AppliedSource(_)
        | OperationState::PlannedKnowledge(_)
        | OperationState::AppliedKnowledge(_) => Err(KbError::invalid_config(
            "operation",
            "expected adoption plan",
        )),
    }
}

fn finish_adoption(user_paths: &UserPaths, plan: &AdoptionPlan) -> Result<AdoptionResult, KbError> {
    let mut warnings = Vec::new();
    if let Err(error) = register_vault(user_paths, &plan.target) {
        warnings.push(format!(
            "Vault adopted but not registered: {error} Run kb vault register {}.",
            plan.target.display()
        ));
    }
    let mut created = plan
        .creates
        .iter()
        .map(|file| file.relative_path.as_str().to_owned())
        .collect::<Vec<_>>();
    created.extend(EMPTY_DIRECTORIES.iter().map(|path| (*path).to_owned()));
    created.sort();
    created.dedup();
    let result = AdoptionResult {
        operation_id: plan.operation_id,
        vault_id: plan.vault_id,
        target: plan.target.clone(),
        created,
        warnings,
    };
    let receipt_path = vault_receipt_path(plan);
    if let Some(parent) = receipt_path.parent() {
        fs::create_dir_all(parent).map_err(|error| io_error("create directory", parent, &error))?;
    }
    write_json(&receipt_path, &result)?;
    save_result(user_paths, &result)?;
    record_adoption_complete(user_paths, plan.operation_id)?;
    Ok(result)
}

fn install_generated(
    user_paths: &UserPaths,
    operation_id: OperationId,
    target: &Path,
    stage: &Path,
    generated: &[(PortableRelativePath, Vec<u8>)],
    tracking: &mut InstallState,
) -> Result<(), KbError> {
    for relative in generated_directories(generated)? {
        let destination = target.join(relative.to_native_path());
        if !destination.exists() {
            fs::create_dir(&destination)
                .map_err(|error| io_error("create directory", &destination, &error))?;
            tracking.created_directories.push(destination);
            tracking.progress.created_directories.push(relative);
            write_json(&tracking.progress_path, &tracking.progress)?;
        } else if !destination.is_dir() {
            return Err(recovery_error(&destination, "expected a directory"));
        }
    }
    for (relative, _) in generated {
        let source = stage.join(relative.to_native_path());
        let destination = target.join(relative.to_native_path());
        tracking.progress.installed_files.push(relative.clone());
        write_json(&tracking.progress_path, &tracking.progress)?;
        fs::hard_link(&source, &destination)
            .map_err(|error| io_error("install generated file at", &destination, &error))?;
        fs::remove_file(&source)
            .map_err(|error| io_error("remove staged file", &source, &error))?;
        tracking.installed_files.push(destination);
        record_adoption_progress(
            user_paths,
            operation_id,
            tracking.installed_files.len(),
            generated.len(),
        )?;
        if tracking.fail_after_installed_file == Some(tracking.installed_files.len()) {
            return Err(KbError::io_failure(
                "continue injected adoption",
                relative.as_str(),
                "test interruption",
            ));
        }
    }
    Ok(())
}

fn record_adoption_start(user_paths: &UserPaths, plan: &AdoptionPlan) -> Result<(), KbError> {
    crate::operation_events::record_operation_event_now(
        user_paths,
        plan.operation_id,
        OperationEventKind::Applying,
        Some((0, plan.creates.len() as u64)),
        "Vault adoption started.",
    )?;
    Ok(())
}

fn record_adoption_recovery(
    user_paths: &UserPaths,
    operation_id: OperationId,
) -> Result<(), KbError> {
    crate::operation_events::record_operation_event_now(
        user_paths,
        operation_id,
        OperationEventKind::Recovering,
        None,
        "Interrupted Vault adoption is being restored.",
    )?;
    Ok(())
}

fn record_adoption_progress(
    user_paths: &UserPaths,
    operation_id: OperationId,
    completed: usize,
    total: usize,
) -> Result<(), KbError> {
    crate::operation_events::record_operation_event_now(
        user_paths,
        operation_id,
        OperationEventKind::Progress,
        Some((completed as u64, total as u64)),
        "Vault adoption progress was recorded.",
    )?;
    Ok(())
}

fn record_adoption_complete(
    user_paths: &UserPaths,
    operation_id: OperationId,
) -> Result<(), KbError> {
    crate::operation_events::record_operation_event_now(
        user_paths,
        operation_id,
        OperationEventKind::Applied,
        None,
        "Vault adoption is complete.",
    )?;
    Ok(())
}

fn rollback_or_recovery<T>(
    target: &Path,
    stage: &Path,
    generated: &[(PortableRelativePath, Vec<u8>)],
    installed_files: &[PathBuf],
    created_directories: &[PathBuf],
    primary: KbError,
) -> Result<T, KbError> {
    let mut failures = Vec::new();
    for path in installed_files.iter().rev() {
        match expected_bytes(path, target, generated) {
            Some(expected) if file_matches(path, expected).unwrap_or(false) => {
                if let Err(error) = fs::remove_file(path) {
                    failures.push(format!("{}: {error}", path.display()));
                }
            }
            _ => failures.push(format!("{}: generated bytes changed", path.display())),
        }
    }
    for directory in directories_deepest_first(created_directories) {
        if let Err(error) = fs::remove_dir(&directory) {
            if error.kind() != std::io::ErrorKind::NotFound {
                failures.push(format!("{}: {error}", directory.display()));
            }
        }
    }
    if let Err(error) = remove_verified_stage(stage, generated) {
        failures.push(error.to_string());
    }
    if failures.is_empty() {
        Err(primary)
    } else {
        Err(recovery_error(
            target,
            &format!("{primary}; restore also failed: {}", failures.join("; ")),
        ))
    }
}

fn recover_interrupted_apply(
    plan: &AdoptionPlan,
    generated: &[(PortableRelativePath, Vec<u8>)],
    progress: &ApplyProgress,
) -> Result<(), KbError> {
    if progress.stage.exists() {
        remove_verified_stage(&progress.stage, generated)?;
    }
    for relative in &progress.installed_files {
        let expected = generated
            .iter()
            .find(|(candidate, _)| candidate == relative)
            .map(|(_, bytes)| bytes)
            .ok_or_else(|| {
                KbError::invalid_config("operation progress", "unexpected installed path")
            })?;
        let path = plan.target.join(relative.to_native_path());
        if path.exists() {
            if !file_matches(&path, expected)? {
                return Err(recovery_error(&path, "generated file bytes changed"));
            }
            fs::remove_file(&path)
                .map_err(|error| io_error("remove interrupted generated file", &path, &error))?;
        }
    }
    let observed = plan
        .observed_entries
        .iter()
        .filter(|entry| entry.kind == ObservedKind::Directory)
        .map(|entry| plan.target.join(entry.relative_path.to_native_path()))
        .collect::<BTreeSet<_>>();
    let generated_dirs = progress
        .created_directories
        .iter()
        .map(|path| plan.target.join(path.to_native_path()))
        .collect::<Vec<_>>();
    for directory in directories_deepest_first(&generated_dirs) {
        if !observed.contains(&directory) && directory.exists() {
            fs::remove_dir(&directory).map_err(|error| {
                recovery_error(
                    &directory,
                    &format!("cannot remove generated directory safely: {error}"),
                )
            })?;
        }
    }
    Ok(())
}

fn validate_progress(
    plan: &AdoptionPlan,
    progress: &ApplyProgress,
    generated: &[(PortableRelativePath, Vec<u8>)],
) -> Result<(), KbError> {
    let expected_stage = plan.target.join(format!(".kb-adopt-{}", plan.operation_id));
    let allowed_directories = generated_directories(generated)?;
    let files_valid = progress
        .installed_files
        .iter()
        .all(|path| generated.iter().any(|(candidate, _)| candidate == path));
    let directories_valid = progress
        .created_directories
        .iter()
        .all(|path| allowed_directories.contains(path));
    if progress.operation_id != plan.operation_id
        || progress.stage != expected_stage
        || !files_valid
        || !directories_valid
    {
        return Err(KbError::invalid_config(
            "operation progress",
            "progress does not match the adoption plan",
        ));
    }
    Ok(())
}

fn progress_path(user_paths: &UserPaths, operation_id: OperationId) -> PathBuf {
    operation_directory(user_paths, operation_id).join("progress.json")
}

fn remove_progress(path: &Path) -> Result<(), KbError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(io_error("remove operation progress", path, &error)),
    }
}

fn populate_stage(
    stage: &Path,
    generated: &[(PortableRelativePath, Vec<u8>)],
) -> Result<(), KbError> {
    for relative in EMPTY_DIRECTORIES {
        fs::create_dir_all(stage.join(relative))
            .map_err(|error| io_error("create staging directory", &stage.join(relative), &error))?;
    }
    for (relative, bytes) in generated {
        let path = stage.join(relative.to_native_path());
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| io_error("create staging directory", parent, &error))?;
        }
        atomic_replace(&path, bytes)?;
    }
    Ok(())
}

fn remove_verified_stage(
    stage: &Path,
    generated: &[(PortableRelativePath, Vec<u8>)],
) -> Result<(), KbError> {
    if !stage.exists() {
        return Ok(());
    }
    kb_core::ensure_not_link_or_reparse_point(stage)?;
    let observed = snapshot(stage)?;
    let expected_files = generated
        .iter()
        .map(|(path, bytes)| (path.as_str().to_owned(), hash_bytes(bytes)))
        .collect::<BTreeMap<_, _>>();
    let expected_directories = generated_directories(generated)?
        .into_iter()
        .map(|path| path.as_str().to_owned())
        .collect::<BTreeSet<_>>();
    for entry in observed {
        let owned = match entry.kind {
            ObservedKind::File => {
                expected_files.get(entry.relative_path.as_str()) == entry.sha256.as_ref()
            }
            ObservedKind::Directory => expected_directories.contains(entry.relative_path.as_str()),
        };
        if !owned {
            return Err(recovery_error(
                &stage.join(entry.relative_path.to_native_path()),
                "staging content is not owned by this plan",
            ));
        }
    }
    fs::remove_dir_all(stage).map_err(|error| io_error("remove staging directory", stage, &error))
}

fn generated_is_complete(
    target: &Path,
    generated: &[(PortableRelativePath, Vec<u8>)],
) -> Result<bool, KbError> {
    for (relative, expected) in generated {
        let path = target.join(relative.to_native_path());
        if !path.exists() || !file_matches(&path, expected)? {
            return Ok(false);
        }
    }
    Ok(EMPTY_DIRECTORIES
        .iter()
        .all(|relative| target.join(relative).is_dir()))
}

fn verify_observed_entries(root: &Path, expected: &[ObservedEntry]) -> Result<(), KbError> {
    for entry in expected {
        let path = root.join(entry.relative_path.to_native_path());
        kb_core::ensure_not_link_or_reparse_point(&path)?;
        let metadata = fs::metadata(&path).map_err(|error| io_error("inspect", &path, &error))?;
        match entry.kind {
            ObservedKind::Directory if !metadata.is_dir() => return Err(plan_stale(&path)),
            ObservedKind::File if !metadata.is_file() => return Err(plan_stale(&path)),
            ObservedKind::File => {
                if metadata.len() != entry.size || Some(hash_file(&path)?) != entry.sha256 {
                    return Err(plan_stale(&path));
                }
            }
            ObservedKind::Directory => {}
        }
    }
    Ok(())
}

fn snapshot(root: &Path) -> Result<Vec<ObservedEntry>, KbError> {
    fn visit(
        root: &Path,
        directory: &Path,
        output: &mut Vec<ObservedEntry>,
    ) -> Result<(), KbError> {
        let mut entries = fs::read_dir(directory)
            .map_err(|error| io_error("read directory", directory, &error))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| io_error("read directory entry", directory, &error))?;
        entries.sort_by_key(fs::DirEntry::file_name);
        for entry in entries {
            let path = entry.path();
            kb_core::ensure_not_link_or_reparse_point(&path)?;
            let metadata =
                fs::metadata(&path).map_err(|error| io_error("inspect", &path, &error))?;
            let relative =
                PortableRelativePath::from_path(path.strip_prefix(root).map_err(|error| {
                    KbError::invalid_config(path.display().to_string(), error.to_string())
                })?)?;
            if metadata.is_dir() {
                output.push(ObservedEntry {
                    relative_path: relative,
                    kind: ObservedKind::Directory,
                    size: 0,
                    sha256: None,
                });
                visit(root, &path, output)?;
            } else if metadata.is_file() {
                output.push(ObservedEntry {
                    relative_path: relative,
                    kind: ObservedKind::File,
                    size: metadata.len(),
                    sha256: Some(hash_file(&path)?),
                });
            } else {
                return Err(recovery_error(&path, "unsupported filesystem entry"));
            }
        }
        Ok(())
    }

    let mut output = Vec::new();
    visit(root, root, &mut output)?;
    let mut portable = BTreeMap::new();
    for entry in &output {
        let key = portability_key(&entry.relative_path);
        if let Some(previous) = portable.insert(key, entry.relative_path.as_str()) {
            return Err(KbError::new(
                ErrorCode::UnsafePath,
                format!(
                    "Paths {} and {} collide on a supported platform.",
                    previous,
                    entry.relative_path.as_str()
                ),
                false,
                "Rename one path before creating an adoption plan.",
            ));
        }
    }
    output.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
    Ok(output)
}

fn generated_files(vault_id: Uuid) -> Result<Vec<(PortableRelativePath, Vec<u8>)>, KbError> {
    STATIC_FILES
        .iter()
        .map(|(path, contents)| {
            Ok((
                PortableRelativePath::parse(path)?,
                contents.as_bytes().to_vec(),
            ))
        })
        .chain(std::iter::once(Ok((
            PortableRelativePath::parse(".kb/config.yml")?,
            config_yaml(vault_id).into_bytes(),
        ))))
        .collect()
}

fn generated_directories(
    generated: &[(PortableRelativePath, Vec<u8>)],
) -> Result<Vec<PortableRelativePath>, KbError> {
    let mut paths = BTreeSet::new();
    for (relative, _) in generated {
        let mut parent = Path::new(relative.as_str()).parent();
        while let Some(path) = parent {
            if path.as_os_str().is_empty() {
                break;
            }
            paths.insert(PortableRelativePath::from_path(path)?);
            parent = path.parent();
        }
    }
    for relative in EMPTY_DIRECTORIES {
        let mut current = Some(Path::new(relative));
        while let Some(path) = current {
            paths.insert(PortableRelativePath::from_path(path)?);
            current = path
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty());
        }
    }
    let mut paths = paths.into_iter().collect::<Vec<_>>();
    paths.sort_by_key(|path| path.as_str().matches('/').count());
    Ok(paths)
}

fn reject_owned_collisions(target: &Path) -> Result<(), KbError> {
    for relative in [
        ".kb",
        "admission.yml",
        "KB.md",
        "Wiki/index.md",
        "Wiki/log.md",
    ] {
        let path = target.join(relative);
        if path.exists() {
            return Err(KbError::new(
                ErrorCode::UnsafePath,
                format!(
                    "Cannot prove ownership of existing path {}.",
                    path.display()
                ),
                false,
                "Move or rename the conflicting path, then review a new adoption plan.",
            ));
        }
    }
    Ok(())
}

fn verify_planned_files(
    plan: &AdoptionPlan,
    generated: &[(PortableRelativePath, Vec<u8>)],
) -> Result<(), KbError> {
    let actual = generated
        .iter()
        .map(|(path, bytes)| PlannedFile {
            relative_path: path.clone(),
            size: bytes.len() as u64,
            sha256: hash_bytes(bytes),
        })
        .collect::<Vec<_>>();
    if actual == plan.creates {
        Ok(())
    } else {
        Err(KbError::invalid_config(
            "operation plan",
            "planned generated bytes do not match this application version",
        ))
    }
}

fn validate_plan_identity(plan: &AdoptionPlan, operation_id: OperationId) -> Result<(), KbError> {
    if plan.schema_version != CURRENT_SCHEMA_VERSION
        || plan.operation_id != operation_id
        || plan.kind != OperationKind::AdoptVault
    {
        return Err(KbError::invalid_config(
            "operation plan",
            "schema, ID, or operation kind does not match",
        ));
    }
    if !plan.target.is_absolute() || plan.target.parent().is_none() {
        return Err(KbError::invalid_config(
            "operation plan target",
            "target must be an absolute non-root directory",
        ));
    }
    kb_core::ensure_not_link_or_reparse_point(&plan.target)?;
    if !plan.target.is_dir() {
        return Err(KbError::new(
            ErrorCode::PlanStale,
            format!("Adoption target is unavailable: {}", plan.target.display()),
            false,
            "Restore the directory or create a new adoption plan.",
        ));
    }
    Ok(())
}

fn verify_result(plan: &AdoptionPlan, result: &AdoptionResult) -> Result<(), KbError> {
    if result.operation_id != plan.operation_id
        || result.vault_id != plan.vault_id
        || result.target != plan.target
    {
        return Err(KbError::invalid_config(
            "operation result",
            "receipt does not match its plan",
        ));
    }
    Ok(())
}

fn operation_lock(user_paths: &UserPaths) -> Result<File, KbError> {
    fs::create_dir_all(&user_paths.state_dir)
        .map_err(|error| io_error("create state directory", &user_paths.state_dir, &error))?;
    let path = user_paths.state_dir.join("operations.lock");
    let file = File::options()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&path)
        .map_err(|error| io_error("open operation lock", &path, &error))?;
    fs2::FileExt::lock_exclusive(&file)
        .map_err(|error| io_error("lock operations", &path, &error))?;
    Ok(file)
}

fn create_private_directory(path: &Path) -> Result<(), KbError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;

        let mut builder = fs::DirBuilder::new();
        builder.mode(0o700);
        builder
            .create(path)
            .map_err(|error| io_error("create private staging directory", path, &error))
    }
    #[cfg(not(unix))]
    fs::create_dir(path).map_err(|error| io_error("create private staging directory", path, &error))
}

fn expected_bytes<'a>(
    path: &Path,
    target: &Path,
    generated: &'a [(PortableRelativePath, Vec<u8>)],
) -> Option<&'a [u8]> {
    let relative = path.strip_prefix(target).ok()?;
    generated
        .iter()
        .find(|(candidate, _)| candidate.to_native_path() == relative)
        .map(|(_, bytes)| bytes.as_slice())
}

fn file_matches(path: &Path, expected: &[u8]) -> Result<bool, KbError> {
    kb_core::ensure_not_link_or_reparse_point(path)?;
    if !path.is_file() {
        return Ok(false);
    }
    Ok(hash_file(path)? == hash_bytes(expected))
}

fn hash_file(path: &Path) -> Result<String, KbError> {
    let mut file = File::open(path).map_err(|error| io_error("open", path, &error))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 64 * 1024];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|error| io_error("read", path, &error))?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(hex::encode(hasher.finalize()))
}

fn hash_bytes(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn directories_deepest_first(paths: &[PathBuf]) -> Vec<PathBuf> {
    let mut paths = paths.to_vec();
    paths.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
    paths.dedup();
    paths
}

fn vault_receipt_path(plan: &AdoptionPlan) -> PathBuf {
    plan.target
        .join(".kb/runtime/operations")
        .join(plan.operation_id.to_string())
        .join("result.json")
}

fn absolute_path(path: &Path) -> Result<PathBuf, KbError> {
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }
    std::env::current_dir()
        .map(|current| current.join(path))
        .map_err(|error| io_error("resolve", path, &error))
}

fn plan_stale(path: &Path) -> KbError {
    KbError::new(
        ErrorCode::PlanStale,
        format!("Reviewed path changed: {}", path.display()),
        false,
        "Create and review a new adoption plan.",
    )
}

fn recovery_error(path: &Path, reason: &str) -> KbError {
    KbError::new(
        ErrorCode::VaultNeedsRecovery,
        format!("Cannot safely restore {}: {reason}", path.display()),
        false,
        format!("Inspect the exact path {} before retrying.", path.display()),
    )
}

fn io_error(action: &str, path: &Path, error: &std::io::Error) -> KbError {
    KbError::io_failure(action, path.display().to_string(), error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_file_install_interruption_restores_then_retries() {
        let probe = tempfile::tempdir().unwrap();
        let probe_target = probe.path().join("existing");
        fs::create_dir(&probe_target).unwrap();
        let probe_paths = user_paths(probe.path());
        let probe_plan = create_adoption_plan(&probe_target, &probe_paths).unwrap();
        let planned_file_count = probe_plan.creates.len();

        for fail_after in 1..=planned_file_count {
            let temp = tempfile::tempdir().unwrap();
            let target = temp.path().join("existing");
            fs::create_dir_all(target.join("Notes")).unwrap();
            let note = target.join("Notes/keep.md");
            fs::write(&note, b"human text\n").unwrap();
            let paths = user_paths(temp.path());
            let plan = create_adoption_plan(&target, &paths).unwrap();

            let error = apply_operation_inner(&paths, plan.operation_id, Some(fail_after))
                .expect_err("injected interruption must fail");
            assert_eq!(error.code, ErrorCode::IoFailure);
            assert_eq!(fs::read(&note).unwrap(), b"human text\n");
            for generated in &plan.creates {
                assert!(
                    !target
                        .join(generated.relative_path.to_native_path())
                        .exists()
                );
            }

            let result = apply_operation(&paths, plan.operation_id).unwrap();
            assert_eq!(result.operation_id, plan.operation_id);
            assert_eq!(fs::read(&note).unwrap(), b"human text\n");
        }
    }

    fn user_paths(root: &Path) -> UserPaths {
        UserPaths::new(
            root.join("user-config"),
            root.join("user-state"),
            root.join("user-cache"),
        )
    }
}
