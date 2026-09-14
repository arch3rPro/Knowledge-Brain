use std::{collections::BTreeSet, fs, path::Path};

use kb_core::{
    CURRENT_SCHEMA_VERSION, ErrorCode, KbError, OperationId, PortableRelativePath, SchemaVersion,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use uuid::Uuid;

use crate::{
    LockMode, UserPaths, VaultLock, ensure_shared_write_sync_safe,
    operation::{operation_directory, read_json, write_json},
    source_io::safe_path,
    template::{
        ADMISSION_SCHEMA_JSON, CONFIG_SCHEMA_JSON, KB_MD, LEGACY_KB_MD_V1_0, RELEASED_KB_MD_V0_1_X,
        VAULT_TEMPLATE_VERSION, managed_rules, template_manifest_yaml,
    },
    vault::read_vault_identity,
};

const MARKER: &str = ".kb/runtime/vault-upgrade-pending.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VaultUpgradeAction {
    Create,
    Update,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VaultUpgradeWrite {
    pub path: PortableRelativePath,
    pub action: VaultUpgradeAction,
    pub before_sha256: Option<String>,
    pub after_sha256: String,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VaultUpgradeConflict {
    pub path: PortableRelativePath,
    pub reason: String,
    pub next_action: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VaultUpgradePlan {
    pub kind: String,
    pub schema_version: SchemaVersion,
    pub operation_id: OperationId,
    pub vault_id: Uuid,
    pub target: std::path::PathBuf,
    pub app_version: String,
    pub created_at: String,
    pub from_template_version: Option<SchemaVersion>,
    pub to_template_version: SchemaVersion,
    pub writes: Vec<VaultUpgradeWrite>,
    pub diff: String,
    pub conflicts: Vec<VaultUpgradeConflict>,
    pub untouched: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VaultUpgradeResult {
    pub kind: String,
    pub schema_version: SchemaVersion,
    pub operation_id: OperationId,
    pub vault_id: Uuid,
    pub target: std::path::PathBuf,
    pub template_version: SchemaVersion,
    pub changed: Vec<PortableRelativePath>,
}

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

#[derive(Deserialize)]
struct ManifestIdentity {
    schema_version: SchemaVersion,
    template_version: SchemaVersion,
}

/// Create a reviewable plan for product-managed Vault template files.
///
/// User notes, admission rules, topic directories, Wiki content, Git metadata,
/// and Obsidian settings are listed as untouched and are never inferred as
/// product-owned.
///
/// # Errors
///
/// Returns an error when the Vault identity or a managed path cannot be read safely.
pub fn create_vault_upgrade_plan(
    root: &Path,
    paths: &UserPaths,
) -> Result<VaultUpgradePlan, KbError> {
    let identity = read_vault_identity(root)?;
    let manifest_path = safe_path(root, ".kb/template.yml")?;
    let (from_template_version, manifest_conflict) = match read_manifest_version(&manifest_path) {
        Ok(version) => (version, None),
        Err(error) if error.code == ErrorCode::InvalidConfig => (
            None,
            Some(conflict(
                ".kb/template.yml",
                "Template metadata cannot be parsed or uses an unsupported manifest schema.",
                "Restore .kb/template.yml from a known backup or remove only this metadata file, then create a new preview.",
            )?),
        ),
        Err(error) => return Err(error),
    };
    if from_template_version.is_some_and(|version| version > VAULT_TEMPLATE_VERSION) {
        return Err(KbError::new(
            ErrorCode::SchemaTooNew,
            "Vault template metadata is newer than this Knowledge-Brain build.",
            false,
            "Upgrade Knowledge-Brain before changing the Vault template.",
        ));
    }

    let mut writes = Vec::new();
    let mut conflicts = Vec::new();
    conflicts.extend(manifest_conflict);
    plan_rules(root, &mut writes, &mut conflicts)?;
    plan_whole_file(
        root,
        ".kb/schemas/admission.schema.json",
        ADMISSION_SCHEMA_JSON,
        &mut writes,
        &mut conflicts,
    )?;
    plan_whole_file(
        root,
        ".kb/schemas/config.schema.json",
        CONFIG_SCHEMA_JSON,
        &mut writes,
        &mut conflicts,
    )?;
    if conflicts
        .iter()
        .all(|conflict| conflict.path.as_str() != ".kb/template.yml")
    {
        plan_manifest(root, &mut writes, &mut conflicts)?;
    }
    if !conflicts.is_empty() {
        writes.clear();
    }
    writes.sort_by(|left, right| left.path.cmp(&right.path));
    conflicts.sort_by(|left, right| left.path.cmp(&right.path));
    let diff = render_diff(root, &writes)?;

    let plan = VaultUpgradePlan {
        kind: "upgrade_vault".into(),
        schema_version: CURRENT_SCHEMA_VERSION,
        operation_id: OperationId::new(),
        vault_id: identity.vault_id,
        target: root.to_path_buf(),
        app_version: env!("CARGO_PKG_VERSION").into(),
        created_at: OffsetDateTime::now_utc()
            .format(&Rfc3339)
            .map_err(|error| KbError::invalid_config("upgrade time", error.to_string()))?,
        from_template_version,
        to_template_version: VAULT_TEMPLATE_VERSION,
        writes,
        diff,
        conflicts,
        untouched: vec![
            "admission.yml and topic directories".into(),
            "ordinary notes and managed Wiki content".into(),
            ".git and .obsidian".into(),
            "machine-local configuration, state, and cache".into(),
        ],
    };
    save_upgrade_plan(paths, &plan)?;
    Ok(plan)
}

/// Apply one previously reviewed Vault template upgrade plan.
///
/// # Errors
///
/// Rejects conflicts, stale previews, changed identities, unsafe paths, and
/// interrupted writes that cannot be restored without overwriting another edit.
pub fn apply_vault_upgrade(
    paths: &UserPaths,
    operation_id: OperationId,
) -> Result<VaultUpgradeResult, KbError> {
    let directory = operation_directory(paths, operation_id);
    let result_path = directory.join("upgrade-result.json");
    let plan: VaultUpgradePlan = read_json(&directory.join("upgrade-plan.json"))?;
    validate_plan(&directory, operation_id, &plan)?;
    if !plan.conflicts.is_empty() {
        return Err(KbError::new(
            ErrorCode::InvalidConfig,
            "Vault template upgrade has conflicts and cannot be applied.",
            false,
            "Review the reported files, merge product markers manually, then create a new preview.",
        )
        .with_details(serde_json::json!({ "conflicts": plan.conflicts })));
    }
    let _lock = VaultLock::acquire(
        &plan.target,
        LockMode::Exclusive,
        "upgrade Vault template",
        Some(operation_id),
    )?;
    let marker = safe_path(&plan.target, MARKER)?;
    let progress_path = directory.join("upgrade-progress.json");
    if result_path.is_file() {
        let result = read_json(&result_path)?;
        remove_if_present(&marker)?;
        remove_if_present(&progress_path)?;
        return Ok(result);
    }
    recover_if_needed(&plan, &marker, &progress_path)?;
    crate::source_apply::ensure_no_pending(&plan.target)?;
    ensure_shared_write_sync_safe(&plan.target)?;
    let identity = read_vault_identity(&plan.target)?;
    if identity.vault_id != plan.vault_id || identity.schema_version != CURRENT_SCHEMA_VERSION {
        return Err(stale("Vault identity or schema changed after preview."));
    }
    preflight(&plan)?;
    apply_writes(&plan, &marker, &progress_path)?;

    let result = VaultUpgradeResult {
        kind: "upgrade_vault".into(),
        schema_version: CURRENT_SCHEMA_VERSION,
        operation_id,
        vault_id: plan.vault_id,
        target: plan.target.clone(),
        template_version: plan.to_template_version,
        changed: plan.writes.iter().map(|write| write.path.clone()).collect(),
    };
    write_json(&result_path, &result)?;
    #[cfg(test)]
    crash_for_test("receipt");
    remove_if_present(&marker)?;
    remove_if_present(&progress_path)?;
    Ok(result)
}

/// Read one persisted Vault upgrade preview without applying it.
///
/// # Errors
///
/// Returns an error when the preview is missing, modified, or malformed.
pub fn inspect_vault_upgrade_plan(
    paths: &UserPaths,
    operation_id: OperationId,
) -> Result<VaultUpgradePlan, KbError> {
    let directory = operation_directory(paths, operation_id);
    let plan: VaultUpgradePlan = read_json(&directory.join("upgrade-plan.json"))?;
    validate_plan(&directory, operation_id, &plan)?;
    Ok(plan)
}

fn plan_rules(
    root: &Path,
    writes: &mut Vec<VaultUpgradeWrite>,
    conflicts: &mut Vec<VaultUpgradeConflict>,
) -> Result<(), KbError> {
    let path = safe_path(root, "KB.md")?;
    let current = read_optional(&path)?;
    let next = match current.as_deref() {
        None => Some(KB_MD.to_owned()),
        Some(value) if text_equal(value, KB_MD) => None,
        Some(value)
            if [LEGACY_KB_MD_V1_0, RELEASED_KB_MD_V0_1_X]
                .iter()
                .any(|baseline| text_equal(value, baseline)) =>
        {
            Some(KB_MD.to_owned())
        }
        Some(value) if managed_rules(value).is_some() => {
            let expected = managed_rules(KB_MD).unwrap_or_default();
            (managed_rules(value) != Some(expected)).then(|| replace_rules(value, expected))
        }
        Some(_) => {
            conflicts.push(conflict(
                "KB.md",
                "The unmarked rules file differs from every known product baseline.",
                "Preserve your rules and manually add the kb:rules markers around the product-owned block.",
            )?);
            None
        }
    };
    if let Some(content) = next {
        writes.push(write_for("KB.md", current.as_deref(), content)?);
    }
    Ok(())
}

fn plan_whole_file(
    root: &Path,
    relative: &str,
    target: &str,
    writes: &mut Vec<VaultUpgradeWrite>,
    conflicts: &mut Vec<VaultUpgradeConflict>,
) -> Result<(), KbError> {
    let current = read_optional(&safe_path(root, relative)?)?;
    match current.as_deref() {
        None => writes.push(write_for(relative, None, target.to_owned())?),
        Some(value) if text_equal(value, target) => {}
        Some(_) => conflicts.push(conflict(
            relative,
            "The product-managed schema differs from the known baseline.",
            "Restore this schema from the installed Knowledge-Brain version, then create a new preview.",
        )?),
    }
    Ok(())
}

fn plan_manifest(
    root: &Path,
    writes: &mut Vec<VaultUpgradeWrite>,
    conflicts: &mut Vec<VaultUpgradeConflict>,
) -> Result<(), KbError> {
    let relative = ".kb/template.yml";
    let current = read_optional(&safe_path(root, relative)?)?;
    let target = template_manifest_yaml();
    match current.as_deref() {
        None => writes.push(write_for(relative, None, target)?),
        Some(value) if text_equal(value, &target) => {}
        Some(_) => conflicts.push(conflict(
            relative,
            "Template metadata differs from every known product baseline.",
            "Restore a known .kb/template.yml baseline or merge the documented template metadata manually, then create a new preview.",
        )?),
    }
    Ok(())
}

fn write_for(
    path: &str,
    before: Option<&str>,
    content: String,
) -> Result<VaultUpgradeWrite, KbError> {
    Ok(VaultUpgradeWrite {
        path: PortableRelativePath::parse(path)?,
        action: if before.is_some() {
            VaultUpgradeAction::Update
        } else {
            VaultUpgradeAction::Create
        },
        before_sha256: before.map(|value| hash(value.as_bytes())),
        after_sha256: hash(content.as_bytes()),
        content,
    })
}

fn conflict(path: &str, reason: &str, next_action: &str) -> Result<VaultUpgradeConflict, KbError> {
    Ok(VaultUpgradeConflict {
        path: PortableRelativePath::parse(path)?,
        reason: reason.into(),
        next_action: next_action.into(),
    })
}

fn read_manifest_version(path: &Path) -> Result<Option<SchemaVersion>, KbError> {
    let Some(content) = read_optional(path)? else {
        return Ok(None);
    };
    let manifest: ManifestIdentity = serde_yaml_ng::from_str(&content)
        .map_err(|error| KbError::invalid_config(path.display().to_string(), error.to_string()))?;
    if manifest.schema_version != CURRENT_SCHEMA_VERSION {
        return Err(KbError::invalid_config(
            path.display().to_string(),
            "unsupported manifest schema_version",
        ));
    }
    Ok(Some(manifest.template_version))
}

fn replace_rules(original: &str, rules: &str) -> String {
    const START: &str = "<!-- kb:rules:start -->";
    const END: &str = "<!-- kb:rules:end -->";
    let start = original.find(START).unwrap_or_default() + START.len();
    let end = original[start..].find(END).unwrap_or_default() + start;
    format!("{}{}{}", &original[..start], rules, &original[end..])
}

fn read_optional(path: &Path) -> Result<Option<String>, KbError> {
    match fs::read_to_string(path) {
        Ok(content) => Ok(Some(content)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(KbError::io_failure(
            "read",
            path.display().to_string(),
            error.to_string(),
        )),
    }
}

fn render_diff(root: &Path, writes: &[VaultUpgradeWrite]) -> Result<String, KbError> {
    let mut output = String::new();
    for write in writes {
        let before = read_optional(&safe_path(root, write.path.as_str())?)?.unwrap_or_default();
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

fn text_equal(left: &str, right: &str) -> bool {
    left.replace("\r\n", "\n") == right.replace("\r\n", "\n")
}

fn save_upgrade_plan(paths: &UserPaths, plan: &VaultUpgradePlan) -> Result<(), KbError> {
    let directory = operation_directory(paths, plan.operation_id);
    fs::create_dir_all(&directory).map_err(|error| {
        KbError::io_failure("create", directory.display().to_string(), error.to_string())
    })?;
    write_json(&directory.join("upgrade-plan.json"), plan)?;
    let bytes = serde_json::to_vec(plan)
        .map_err(|error| KbError::invalid_config("upgrade plan", error.to_string()))?;
    write_json(&directory.join("upgrade-plan.sha256"), &hash(&bytes))
}

fn validate_plan(
    directory: &Path,
    operation_id: OperationId,
    plan: &VaultUpgradePlan,
) -> Result<(), KbError> {
    let expected: String = read_json(&directory.join("upgrade-plan.sha256"))?;
    let bytes = serde_json::to_vec(plan)
        .map_err(|error| KbError::invalid_config("upgrade plan", error.to_string()))?;
    let mut paths = BTreeSet::new();
    let allowed = [
        "KB.md",
        ".kb/schemas/admission.schema.json",
        ".kb/schemas/config.schema.json",
        ".kb/template.yml",
    ];
    if expected != hash(&bytes)
        || plan.operation_id != operation_id
        || plan.kind != "upgrade_vault"
        || plan.schema_version != CURRENT_SCHEMA_VERSION
        || plan.to_template_version != VAULT_TEMPLATE_VERSION
        || plan.app_version != env!("CARGO_PKG_VERSION")
        || !plan.target.is_absolute()
        || plan.writes.iter().any(|write| {
            !allowed.contains(&write.path.as_str())
                || write.after_sha256 != hash(write.content.as_bytes())
                || !paths.insert(write.path.clone())
        })
    {
        return Err(KbError::invalid_config(
            "Vault upgrade plan",
            "identity, version, digest, or write set is invalid",
        ));
    }
    Ok(())
}

fn preflight(plan: &VaultUpgradePlan) -> Result<(), KbError> {
    for write in &plan.writes {
        let path = safe_path(&plan.target, write.path.as_str())?;
        let current = read_optional(&path)?.map(|content| hash(content.as_bytes()));
        if current != write.before_sha256 {
            return Err(stale(format!(
                "{} changed after preview.",
                write.path.as_str()
            )));
        }
    }
    if render_diff(&plan.target, &plan.writes)? != plan.diff {
        return Err(KbError::invalid_config(
            "Vault upgrade plan",
            "review diff does not match the planned writes",
        ));
    }
    Ok(())
}

fn apply_writes(
    plan: &VaultUpgradePlan,
    marker: &Path,
    progress_path: &Path,
) -> Result<(), KbError> {
    let mut progress = Progress {
        operation_id: plan.operation_id,
        entries: Vec::new(),
    };
    write_json(progress_path, &progress)?;
    write_json(marker, &plan.operation_id)?;
    #[cfg(test)]
    crash_for_test("marker");
    let outcome = (|| {
        for write in &plan.writes {
            let path = safe_path(&plan.target, write.path.as_str())?;
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).map_err(|error| {
                    KbError::io_failure("create", parent.display().to_string(), error.to_string())
                })?;
            }
            let before = match fs::read(&path) {
                Ok(bytes) => Some(bytes),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                Err(error) => {
                    return Err(KbError::io_failure(
                        "read",
                        path.display().to_string(),
                        error.to_string(),
                    ));
                }
            };
            progress.entries.push(Effect {
                path: write.path.clone(),
                before,
                after_sha256: write.after_sha256.clone(),
            });
            write_json(progress_path, &progress)?;
            if write.before_sha256.is_some() {
                crate::atomic_replace(&path, write.content.as_bytes())?;
            } else {
                crate::storage::create_new(&path, write.content.as_bytes())?;
            }
            #[cfg(test)]
            crash_for_test(&format!("write-{}", progress.entries.len()));
        }
        Ok(())
    })();
    if let Err(error) = outcome {
        restore(plan, &progress)?;
        remove_if_present(progress_path)?;
        remove_if_present(marker)?;
        return Err(error);
    }
    Ok(())
}

fn recover_if_needed(
    plan: &VaultUpgradePlan,
    marker: &Path,
    progress_path: &Path,
) -> Result<(), KbError> {
    if !marker.exists() && !progress_path.exists() {
        return Ok(());
    }
    let pending: OperationId = read_json(marker)?;
    if pending != plan.operation_id {
        return Err(recovery("Another Vault upgrade is pending."));
    }
    let progress: Progress = read_json(progress_path)?;
    if progress.operation_id != plan.operation_id {
        return Err(recovery("Vault upgrade progress identity mismatch."));
    }
    restore(plan, &progress)?;
    remove_if_present(progress_path)?;
    remove_if_present(marker)
}

fn restore(plan: &VaultUpgradePlan, progress: &Progress) -> Result<(), KbError> {
    for effect in progress.entries.iter().rev() {
        let path = safe_path(&plan.target, effect.path.as_str())?;
        let current = match fs::read(&path) {
            Ok(bytes) => Some(hash(&bytes)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => {
                return Err(KbError::io_failure(
                    "read",
                    path.display().to_string(),
                    error.to_string(),
                ));
            }
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
            crate::atomic_replace(&path, bytes)?;
        } else {
            fs::remove_file(&path).map_err(|error| {
                KbError::io_failure("remove", path.display().to_string(), error.to_string())
            })?;
        }
    }
    Ok(())
}

fn remove_if_present(path: &Path) -> Result<(), KbError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(KbError::io_failure(
            "remove",
            path.display().to_string(),
            error.to_string(),
        )),
    }
}

fn hash(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn stale(message: impl Into<String>) -> KbError {
    KbError::new(
        ErrorCode::PlanStale,
        message,
        false,
        "Create and review a new kb vault upgrade preview.",
    )
}

fn recovery(message: impl Into<String>) -> KbError {
    KbError::new(
        ErrorCode::VaultNeedsRecovery,
        message,
        false,
        "Retry the same kb vault upgrade confirmation and preserve independently changed files.",
    )
}

#[cfg(test)]
fn crash_for_test(point: &str) {
    if std::env::var("KB_VAULT_UPGRADE_CRASH_POINT").as_deref() == Ok(point) {
        std::process::exit(89);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{InitRequest, init_vault};

    fn paths(base: &Path) -> UserPaths {
        UserPaths::new(base.join("config"), base.join("state"), base.join("cache"))
    }

    #[test]
    fn crash_child() {
        let Ok(base) = std::env::var("KB_VAULT_UPGRADE_CHILD_ROOT") else {
            return;
        };
        let operation_id = std::env::var("KB_VAULT_UPGRADE_CHILD_ID")
            .unwrap()
            .parse()
            .unwrap();
        apply_vault_upgrade(&paths(Path::new(&base)), operation_id).unwrap();
        panic!("child did not reach requested crash point");
    }

    #[test]
    fn process_exit_at_each_write_boundary_is_recoverable() {
        for point in ["marker", "write-1", "write-2", "receipt"] {
            let temporary = tempfile::tempdir().unwrap();
            let root = temporary.path().join("vault");
            let user_paths = paths(temporary.path());
            init_vault(&InitRequest {
                target: root.clone(),
            })
            .unwrap();
            fs::remove_file(root.join(".kb/template.yml")).unwrap();
            fs::write(root.join("KB.md"), LEGACY_KB_MD_V1_0).unwrap();
            let plan = create_vault_upgrade_plan(&root, &user_paths).unwrap();
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "vault_upgrade::tests::crash_child",
                    "--nocapture",
                ])
                .env("KB_VAULT_UPGRADE_CHILD_ROOT", temporary.path())
                .env("KB_VAULT_UPGRADE_CHILD_ID", plan.operation_id.to_string())
                .env("KB_VAULT_UPGRADE_CRASH_POINT", point)
                .output()
                .unwrap();
            assert_eq!(output.status.code(), Some(89));
            assert_eq!(crate::status::pending_operations(&user_paths, &root), 1);

            let result = apply_vault_upgrade(&user_paths, plan.operation_id).unwrap();

            assert_eq!(result.template_version, VAULT_TEMPLATE_VERSION);
            assert_eq!(fs::read_to_string(root.join("KB.md")).unwrap(), KB_MD);
            assert!(!root.join(MARKER).exists());
        }
    }
}
