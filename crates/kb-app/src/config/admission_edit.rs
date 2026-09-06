use std::{fs, path::Path, str::FromStr};

use kb_core::{AdmissionDocument, KbError};
use yaml_edit::{Mapping, YamlFile};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdmissionAction {
    Add { id: String, path: String },
    Enable { id: String },
    Disable { id: String },
    Remove { id: String },
}

/// Parse and validate the Vault admission document.
///
/// # Errors
///
/// Returns `KbError` when the file cannot be read, parsed, or validated against
/// the Vault boundary.
pub fn load_admission(vault_root: &Path) -> Result<AdmissionDocument, KbError> {
    let path = vault_root.join("admission.yml");
    let bytes = fs::read(&path).map_err(|error| {
        KbError::io_failure("read", path.display().to_string(), error.to_string())
    })?;
    let document: AdmissionDocument = serde_yaml_ng::from_slice(&bytes)
        .map_err(|error| KbError::invalid_config("admission.yml", error.to_string()))?;
    document.validate(vault_root)?;
    Ok(document)
}

/// Edit one admission entry while preserving unrelated YAML syntax.
///
/// # Errors
///
/// Returns `KbError` for malformed YAML, a missing or duplicate ID, an invalid
/// action, or a candidate document that violates admission rules.
pub fn edit_admission(
    input: &str,
    vault_root: &Path,
    action: &AdmissionAction,
) -> Result<String, KbError> {
    let parsed: AdmissionDocument = serde_yaml_ng::from_str(input)
        .map_err(|error| KbError::invalid_config("admission.yml", error.to_string()))?;
    parsed.validate(vault_root)?;

    let file = YamlFile::from_str(input)
        .map_err(|error| KbError::invalid_config("admission.yml", error.to_string()))?;
    if file.documents().count() != 1 {
        return Err(KbError::invalid_config(
            "admission.yml",
            "expected one YAML document",
        ));
    }
    let document = file
        .document()
        .ok_or_else(|| KbError::invalid_config("admission.yml", "document is empty"))?;
    let sequence = document.get_sequence("directories").ok_or_else(|| {
        KbError::invalid_config("admission.yml", "directories must be a sequence")
    })?;

    match action {
        AdmissionAction::Add { id, path } => {
            if find_entry_index(&sequence, id)?.is_some() {
                return Err(KbError::invalid_config(
                    "admission.yml",
                    format!("admission id already exists: {id}"),
                ));
            }
            // A flow mapping avoids an upstream indentation ambiguity when a
            // detached block mapping is inserted into a nested empty sequence.
            // JSON object syntax is valid YAML and remains directly editable.
            let quoted_id = serde_json::to_string(id)
                .map_err(|error| KbError::invalid_config("admission.yml", error.to_string()))?;
            let quoted_path = serde_json::to_string(path)
                .map_err(|error| KbError::invalid_config("admission.yml", error.to_string()))?;
            let entry = format!("{{ id: {quoted_id}, path: {quoted_path}, enabled: true }}");
            let entry_file = YamlFile::from_str(&entry)
                .map_err(|error| KbError::invalid_config("admission.yml", error.to_string()))?;
            let mapping = entry_file
                .document()
                .and_then(|document| document.as_mapping())
                .ok_or_else(|| {
                    KbError::invalid_config("admission.yml", "cannot build admission entry")
                })?;
            sequence.push(mapping);
        }
        AdmissionAction::Enable { id } => {
            entry_mapping(&sequence, id)?.set("enabled", true);
        }
        AdmissionAction::Disable { id } => {
            entry_mapping(&sequence, id)?.set("enabled", false);
        }
        AdmissionAction::Remove { id } => {
            let index = find_entry_index(&sequence, id)?.ok_or_else(|| missing_id(id))?;
            sequence.remove(index).ok_or_else(|| missing_id(id))?;
        }
    }

    let output = file
        .to_string()
        .replace("directories:  []", "directories: []");
    let candidate: AdmissionDocument = serde_yaml_ng::from_str(&output)
        .map_err(|error| KbError::invalid_config("admission.yml", error.to_string()))?;
    candidate.validate(vault_root)?;
    Ok(output)
}

fn entry_mapping(sequence: &yaml_edit::Sequence, id: &str) -> Result<Mapping, KbError> {
    let index = find_entry_index(sequence, id)?.ok_or_else(|| missing_id(id))?;
    sequence
        .get(index)
        .and_then(|node| node.as_mapping().cloned())
        .ok_or_else(|| KbError::invalid_config("admission.yml", "entry must be a mapping"))
}

fn find_entry_index(sequence: &yaml_edit::Sequence, id: &str) -> Result<Option<usize>, KbError> {
    let mut found = None;
    for (index, node) in sequence.values().enumerate() {
        let mapping = node
            .as_mapping()
            .ok_or_else(|| KbError::invalid_config("admission.yml", "entry must be a mapping"))?;
        let entry_id = mapping
            .get("id")
            .and_then(|value| value.as_scalar().map(yaml_edit::Scalar::as_string))
            .ok_or_else(|| KbError::invalid_config("admission.yml", "entry id must be a scalar"))?;
        if entry_id == id {
            if found.is_some() {
                return Err(KbError::invalid_config(
                    "admission.yml",
                    format!("admission id occurs more than once: {id}"),
                ));
            }
            found = Some(index);
        }
    }
    Ok(found)
}

fn missing_id(id: &str) -> KbError {
    KbError::invalid_config(
        "admission.yml",
        format!("admission id does not exist: {id}"),
    )
}
