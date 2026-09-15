use std::{collections::BTreeMap, fs, path::Path};

use kb_core::{
    CURRENT_SCHEMA_VERSION, ErrorCode, KbError, PortableRelativePath, SchemaVersion,
    UpdateComponentState, UpdateConflict, UpdateFileAction, UpdateFileChange, UpdateOwnership,
};
use serde::{Deserialize, Serialize};

use crate::{
    atomic_replace,
    source_io::safe_path,
    storage::create_new,
    template::{
        ADMISSION_SCHEMA_JSON, CONFIG_SCHEMA_JSON, HISTORICAL_KB_MD_V1_1,
        HISTORICAL_TEMPLATE_MANIFEST_V1_1, KB_MD, LEGACY_KB_MD_V1_0, ManagedTemplateOwnership,
        RELEASED_KB_MD_V0_1_X, RULES_END_MARKER, RULES_START_MARKER, TemplateManifest,
        VAULT_TEMPLATE_VERSION, current_template_manifest, hash, parse_managed_rules,
        template_manifest_yaml,
    },
    vault::read_vault_identity,
};

const TEMPLATE_COMPONENT_ID: &str = "vault-template";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TemplateCompatibility {
    Current,
    Outdated,
    Modified,
    MissingManifest,
    Unknown,
    Malformed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TemplateInspection {
    pub version: Option<SchemaVersion>,
    pub latest: SchemaVersion,
    pub compatibility: TemplateCompatibility,
    pub upgrade_available: bool,
    pub modified_entries: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TemplateWrite {
    pub path: PortableRelativePath,
    pub before_sha256: Option<String>,
    pub after_sha256: String,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TemplateUpdatePlan {
    pub component_state: UpdateComponentState,
    pub compatibility: TemplateCompatibility,
    pub from_template_version: Option<SchemaVersion>,
    pub to_template_version: SchemaVersion,
    pub writes: Vec<TemplateWrite>,
    pub changes: Vec<UpdateFileChange>,
    pub conflicts: Vec<UpdateConflict>,
    pub unchanged: Vec<PortableRelativePath>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TemplateUpdateResult {
    pub from_template_version: Option<SchemaVersion>,
    pub to_template_version: SchemaVersion,
    pub changed: Vec<PortableRelativePath>,
    pub unchanged: Vec<PortableRelativePath>,
    pub stale: Vec<PortableRelativePath>,
    pub skipped: Vec<PortableRelativePath>,
    pub failed: Vec<PortableRelativePath>,
}

/// Inspect product-owned Vault template content without changing it.
///
/// # Errors
///
/// Returns an error when the Vault or a managed path cannot be read safely.
pub fn inspect_template(root: &Path) -> Result<TemplateInspection, KbError> {
    let plan = plan_template_update(root)?;
    Ok(TemplateInspection {
        version: plan.from_template_version,
        latest: plan.to_template_version,
        compatibility: plan.compatibility,
        upgrade_available: !plan.writes.is_empty(),
        modified_entries: plan
            .conflicts
            .iter()
            .filter_map(|conflict| conflict.path.as_ref())
            .map(|path| path.to_string_lossy().into_owned())
            .collect(),
    })
}

/// Build an all-or-skip update plan for one Vault's product template.
///
/// # Errors
///
/// Returns an error for unreadable/unsafe paths or a manifest newer than this build.
pub fn plan_template_update(root: &Path) -> Result<TemplateUpdatePlan, KbError> {
    read_vault_identity(root)?;
    let manifest_path = safe_path(root, ".kb/template.yml")?;
    let manifest_text = read_optional(&manifest_path)?;
    let manifest = match manifest_text.as_deref() {
        Some(text) => match serde_yaml_ng::from_str::<TemplateManifest>(text) {
            Ok(manifest) => Some(manifest),
            Err(error) => {
                return Ok(conflicted_plan(
                    None,
                    TemplateCompatibility::Malformed,
                    conflict(
                        ".kb/template.yml",
                        format!("Template metadata cannot be parsed: {error}"),
                        "Restore .kb/template.yml from a known Knowledge-Brain baseline, or remove only this metadata file and preview again.",
                    ),
                ));
            }
        },
        None => None,
    };
    if let Some(manifest) = &manifest {
        if manifest.template_version > VAULT_TEMPLATE_VERSION {
            return Err(KbError::new(
                ErrorCode::SchemaTooNew,
                "Vault template metadata is newer than this Knowledge-Brain build.",
                false,
                "Upgrade Knowledge-Brain before changing the Vault template.",
            ));
        }
        if let Err(error) = validate_manifest(manifest) {
            return Ok(conflicted_plan(
                Some(manifest.template_version),
                TemplateCompatibility::Malformed,
                conflict(
                    ".kb/template.yml",
                    format!("Template metadata is malformed: {error}"),
                    "Restore .kb/template.yml from a known Knowledge-Brain baseline, then preview again.",
                ),
            ));
        }
        if !is_recognized_manifest(manifest) {
            return Ok(conflicted_plan(
                Some(manifest.template_version),
                TemplateCompatibility::Unknown,
                conflict(
                    ".kb/template.yml",
                    "Template metadata does not match a recognized Knowledge-Brain ownership baseline.",
                    "Preserve the Vault and restore a recognized .kb/template.yml baseline before previewing again.",
                ),
            ));
        }
    }

    let mut plan = TemplateUpdatePlan {
        component_state: UpdateComponentState::Unchanged,
        compatibility: TemplateCompatibility::Current,
        from_template_version: manifest.as_ref().map(|value| value.template_version),
        to_template_version: VAULT_TEMPLATE_VERSION,
        writes: Vec::new(),
        changes: Vec::new(),
        conflicts: Vec::new(),
        unchanged: Vec::new(),
    };
    if let Some(manifest) = &manifest {
        plan_from_manifest(root, manifest, &mut plan)?;
    } else {
        plan_without_manifest(root, &mut plan)?;
    }
    if !plan.conflicts.is_empty() {
        plan.writes.clear();
        plan.changes.clear();
        plan.component_state = UpdateComponentState::Skipped;
        plan.compatibility = TemplateCompatibility::Modified;
    } else if !plan.writes.is_empty() {
        plan.component_state = UpdateComponentState::Pending;
        plan.compatibility = if plan.from_template_version.is_some() {
            TemplateCompatibility::Outdated
        } else {
            TemplateCompatibility::MissingManifest
        };
    } else if plan.from_template_version.is_none() {
        plan.compatibility = TemplateCompatibility::MissingManifest;
    }
    plan.writes
        .sort_by(|left, right| left.path.cmp(&right.path));
    plan.changes
        .sort_by(|left, right| left.path.cmp(&right.path));
    plan.unchanged.sort();
    plan.conflicts
        .sort_by(|left, right| left.path.cmp(&right.path));
    Ok(plan)
}

/// Apply one already-reviewed template plan after digest revalidation.
///
/// # Errors
///
/// Returns a stale-plan error if any target changed, and restores in-process partial writes.
pub fn apply_template_update(
    root: &Path,
    plan: &TemplateUpdatePlan,
) -> Result<TemplateUpdateResult, KbError> {
    if !plan.conflicts.is_empty() || plan.component_state == UpdateComponentState::Skipped {
        return Err(KbError::invalid_config(
            "Vault template update",
            "a skipped template component cannot be applied",
        ));
    }
    let mut originals = Vec::new();
    for write in &plan.writes {
        let path = safe_path(root, write.path.as_str())?;
        let before = read_optional(&path)?;
        if before.as_deref().map(|value| hash(value.as_bytes())) != write.before_sha256 {
            return Err(stale(&write.path));
        }
        originals.push((path, before));
    }
    let mut applied = 0;
    for (write, (path, _)) in plan.writes.iter().zip(&originals) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                KbError::io_failure("create", parent.display().to_string(), error.to_string())
            })?;
        }
        let result = if write.before_sha256.is_some() {
            atomic_replace(path, write.content.as_bytes())
        } else {
            create_new(path, write.content.as_bytes())
        };
        if let Err(error) = result {
            restore_applied(&originals[..applied])?;
            return Err(error);
        }
        applied += 1;
    }
    Ok(TemplateUpdateResult {
        from_template_version: plan.from_template_version,
        to_template_version: plan.to_template_version,
        changed: plan.writes.iter().map(|write| write.path.clone()).collect(),
        unchanged: plan.unchanged.clone(),
        stale: Vec::new(),
        skipped: Vec::new(),
        failed: Vec::new(),
    })
}

fn plan_from_manifest(
    root: &Path,
    manifest: &TemplateManifest,
    plan: &mut TemplateUpdatePlan,
) -> Result<(), KbError> {
    let recorded = manifest
        .managed
        .iter()
        .map(|entry| (entry.path.as_str(), entry))
        .collect::<BTreeMap<_, _>>();
    for target in current_template_manifest().managed {
        let Some(old) = recorded.get(target.path.as_str()) else {
            let current = read_optional(&safe_path(root, &target.path)?)?;
            if current.is_none() {
                plan_target_write(
                    root,
                    &target.path,
                    None,
                    target_content(&target.path)?,
                    plan,
                )?;
            } else {
                plan.conflicts.push(conflict(
                    &target.path,
                    "A target template path exists without a recorded ownership baseline.",
                    "Preserve the file and resolve ownership manually before previewing again.",
                ));
            }
            continue;
        };
        let current = read_optional(&safe_path(root, &target.path)?)?;
        match old.ownership {
            ManagedTemplateOwnership::MarkedRegion => {
                let Some(text) = current.as_deref() else {
                    plan.conflicts
                        .push(rules_conflict("The managed rules file is missing."));
                    continue;
                };
                let region = match parse_managed_rules(text) {
                    Ok(region) => region,
                    Err(_) => {
                        plan.conflicts.push(rules_conflict(
                            "The rules markers are missing, duplicated, or reversed.",
                        ));
                        continue;
                    }
                };
                if hash(region.content.as_bytes()) != old.sha256 {
                    plan.conflicts.push(rules_conflict(
                        "The product-managed rules region was customized after installation.",
                    ));
                    continue;
                }
                let target_rules = parse_managed_rules(KB_MD)
                    .expect("embedded KB.md has valid markers")
                    .content;
                if hash(target_rules.as_bytes()) == old.sha256 {
                    plan.unchanged.push(PortableRelativePath::parse("KB.md")?);
                } else {
                    let content = format!(
                        "{}{}{}",
                        &text[..region.content_start],
                        target_rules,
                        &text[region.content_end..]
                    );
                    plan_target_write(root, "KB.md", current.as_deref(), content, plan)?;
                }
            }
            ManagedTemplateOwnership::WholeFile => match current.as_deref() {
                None => plan_target_write(
                    root,
                    &target.path,
                    None,
                    target_content(&target.path)?,
                    plan,
                )?,
                Some(text) if hash(text.as_bytes()) != old.sha256 => {
                    plan.conflicts.push(conflict(
                        &target.path,
                        "The product-managed file differs from its recorded installation baseline.",
                        "Preserve the customized file and restore or reconcile it manually before previewing again.",
                    ));
                }
                Some(_text) if old.sha256 == target.sha256 => {
                    plan.unchanged
                        .push(PortableRelativePath::parse(&target.path)?);
                }
                Some(text) => plan_target_write(
                    root,
                    &target.path,
                    Some(text),
                    target_content(&target.path)?,
                    plan,
                )?,
            },
        }
    }
    if plan.conflicts.is_empty()
        && (manifest != &current_template_manifest() || !plan.writes.is_empty())
    {
        plan_target_write(
            root,
            ".kb/template.yml",
            read_optional(&safe_path(root, ".kb/template.yml")?)?.as_deref(),
            template_manifest_yaml(),
            plan,
        )?;
    }
    Ok(())
}

fn plan_without_manifest(root: &Path, plan: &mut TemplateUpdatePlan) -> Result<(), KbError> {
    let rules = read_optional(&safe_path(root, "KB.md")?)?;
    match rules.as_deref() {
        None => plan_target_write(root, "KB.md", None, KB_MD.to_owned(), plan)?,
        Some(text) if text_equal(text, KB_MD) => {
            plan.unchanged.push(PortableRelativePath::parse("KB.md")?);
        }
        Some(text)
            if [LEGACY_KB_MD_V1_0, RELEASED_KB_MD_V0_1_X]
                .iter()
                .any(|known| text_equal(text, known)) =>
        {
            plan_target_write(root, "KB.md", Some(text), KB_MD.to_owned(), plan)?;
        }
        Some(text) => match parse_managed_rules(text) {
            Ok(region) if known_managed_rules_digest(&hash(region.content.as_bytes())) => {
                plan.unchanged.push(PortableRelativePath::parse("KB.md")?);
            }
            _ => plan.conflicts.push(rules_conflict(
                "The customized rules file has no known product ownership baseline.",
            )),
        },
    }
    for relative in [
        ".kb/schemas/admission.schema.json",
        ".kb/schemas/config.schema.json",
    ] {
        let target = target_content(relative)?;
        let current = read_optional(&safe_path(root, relative)?)?;
        match current.as_deref() {
            None => plan_target_write(root, relative, None, target, plan)?,
            Some(text) if text_equal(text, &target) => {
                plan.unchanged.push(PortableRelativePath::parse(relative)?);
            }
            Some(_) => plan.conflicts.push(conflict(
                relative,
                "The product schema differs from every known baseline.",
                "Preserve the file and restore a known Knowledge-Brain schema before previewing again.",
            )),
        }
    }
    if plan.conflicts.is_empty() {
        plan_target_write(
            root,
            ".kb/template.yml",
            None,
            template_manifest_yaml(),
            plan,
        )?;
    }
    Ok(())
}

fn validate_manifest(manifest: &TemplateManifest) -> Result<(), KbError> {
    let expected = current_template_manifest()
        .managed
        .into_iter()
        .map(|entry| (entry.path, entry.ownership))
        .collect::<BTreeMap<_, _>>();
    let mut found = BTreeMap::new();
    for entry in &manifest.managed {
        if entry.sha256.len() != 64
            || !entry
                .sha256
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            || expected.get(&entry.path) != Some(&entry.ownership)
            || found.insert(entry.path.clone(), entry.ownership).is_some()
        {
            return Err(KbError::invalid_config(
                ".kb/template.yml",
                "managed paths, ownership, or digests are invalid",
            ));
        }
    }
    if manifest.schema_version != CURRENT_SCHEMA_VERSION {
        return Err(KbError::invalid_config(
            ".kb/template.yml",
            "unsupported manifest schema_version",
        ));
    }
    Ok(())
}

fn is_recognized_manifest(manifest: &TemplateManifest) -> bool {
    manifest == &current_template_manifest()
        || serde_yaml_ng::from_str::<TemplateManifest>(HISTORICAL_TEMPLATE_MANIFEST_V1_1)
            .is_ok_and(|known| manifest == &known)
}

fn known_managed_rules_digest(digest: &str) -> bool {
    let current = current_template_manifest().managed[0].sha256.clone();
    let historical = parse_managed_rules(HISTORICAL_KB_MD_V1_1)
        .map(|region| hash(region.content.as_bytes()))
        .expect("historical KB.md has valid markers");
    digest == current || digest == historical
}

fn plan_target_write(
    _root: &Path,
    relative: &str,
    before: Option<&str>,
    content: String,
    plan: &mut TemplateUpdatePlan,
) -> Result<(), KbError> {
    let path = PortableRelativePath::parse(relative)?;
    let before_sha256 = before.map(|value| hash(value.as_bytes()));
    let after_sha256 = hash(content.as_bytes());
    let action = if before.is_some() {
        UpdateFileAction::Update
    } else {
        UpdateFileAction::Create
    };
    plan.changes.push(UpdateFileChange {
        path: path.as_str().into(),
        ownership: if relative == "KB.md" {
            UpdateOwnership::MarkedRegion
        } else {
            UpdateOwnership::WholeFile
        },
        action,
        before_sha256: before_sha256.clone(),
        after_sha256: Some(after_sha256.clone()),
        diff: Some(render_diff(relative, before.unwrap_or_default(), &content)),
    });
    plan.writes.push(TemplateWrite {
        path,
        before_sha256,
        after_sha256,
        content,
    });
    Ok(())
}

fn target_content(relative: &str) -> Result<String, KbError> {
    match relative {
        "KB.md" => Ok(KB_MD.to_owned()),
        ".kb/schemas/admission.schema.json" => Ok(ADMISSION_SCHEMA_JSON.to_owned()),
        ".kb/schemas/config.schema.json" => Ok(CONFIG_SCHEMA_JSON.to_owned()),
        ".kb/template.yml" => Ok(template_manifest_yaml()),
        _ => Err(KbError::invalid_config(
            "template target",
            format!("unsupported managed path {relative}"),
        )),
    }
}

fn render_diff(path: &str, before: &str, after: &str) -> String {
    let mut output = format!("--- {path}\n+++ {path}\n@@ complete file @@\n");
    for line in before.lines() {
        output.push_str(&format!("-{line}\n"));
    }
    for line in after.lines() {
        output.push_str(&format!("+{line}\n"));
    }
    output
}

fn rules_conflict(reason: &str) -> UpdateConflict {
    conflict(
        "KB.md",
        reason,
        format!(
            "Keep exactly one ordered marker pair:\n{RULES_START_MARKER}\nproduct-managed rules\n{RULES_END_MARKER}\nContent outside these markers is preserved."
        ),
    )
}

fn conflict(
    path: &str,
    reason: impl Into<String>,
    next_action: impl Into<String>,
) -> UpdateConflict {
    UpdateConflict {
        component_id: TEMPLATE_COMPONENT_ID.into(),
        path: Some(path.into()),
        reason: reason.into(),
        next_action: next_action.into(),
    }
}

fn conflicted_plan(
    from_template_version: Option<SchemaVersion>,
    compatibility: TemplateCompatibility,
    conflict: UpdateConflict,
) -> TemplateUpdatePlan {
    TemplateUpdatePlan {
        component_state: UpdateComponentState::Skipped,
        compatibility,
        from_template_version,
        to_template_version: VAULT_TEMPLATE_VERSION,
        writes: Vec::new(),
        changes: Vec::new(),
        conflicts: vec![conflict],
        unchanged: Vec::new(),
    }
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

fn restore_applied(originals: &[(std::path::PathBuf, Option<String>)]) -> Result<(), KbError> {
    for (path, original) in originals.iter().rev() {
        if let Some(content) = original {
            atomic_replace(path, content.as_bytes())?;
        } else {
            match fs::remove_file(path) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    return Err(KbError::io_failure(
                        "remove",
                        path.display().to_string(),
                        error.to_string(),
                    ));
                }
            }
        }
    }
    Ok(())
}

fn stale(path: &PortableRelativePath) -> KbError {
    KbError::new(
        ErrorCode::PlanStale,
        format!("{} changed after preview.", path.as_str()),
        false,
        "Create and review a new update preview.",
    )
}

fn text_equal(left: &str, right: &str) -> bool {
    left.replace("\r\n", "\n") == right.replace("\r\n", "\n")
}
