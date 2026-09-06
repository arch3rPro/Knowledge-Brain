use std::str::FromStr;

use kb_core::KbError;
use yaml_edit::{Document, YamlFile, path::YamlPath};

/// Edit one supported scalar key while retaining the surrounding YAML syntax.
///
/// # Errors
///
/// Returns `KbError` when the document is malformed, the key is unsupported,
/// a target mapping contains duplicate keys, or the value has the wrong type.
pub fn edit_config(input: &str, key: &str, value: Option<&str>) -> Result<String, KbError> {
    serde_yaml_ng::from_str::<serde_yaml_ng::Value>(input)
        .map_err(|error| KbError::invalid_config("configuration", error.to_string()))?;
    validate_supported_key(key)?;

    let file = YamlFile::from_str(input)
        .map_err(|error| KbError::invalid_config("configuration", error.to_string()))?;
    let documents = file.documents().count();
    if documents != 1 {
        return Err(KbError::invalid_config(
            "configuration",
            format!("expected one YAML document, found {documents}"),
        ));
    }
    let document = file
        .document()
        .ok_or_else(|| KbError::invalid_config("configuration", "document is empty"))?;
    reject_duplicate_target(&document, key)?;

    match value {
        Some(value) => set_typed_value(&document, key, value)?,
        None => {
            if document.try_get_path(key).is_ok() {
                document.try_remove_path(key).map_err(|error| {
                    KbError::invalid_config(key, format!("cannot remove value: {error}"))
                })?;
            }
        }
    }

    let output = file.to_string();
    serde_yaml_ng::from_str::<serde_yaml_ng::Value>(&output)
        .map_err(|error| KbError::invalid_config("edited configuration", error.to_string()))?;
    Ok(output)
}

fn validate_supported_key(key: &str) -> Result<(), KbError> {
    match key {
        "search.mode"
        | "limits.max_file_bytes"
        | "limits.max_files_per_review"
        | "limits.max_total_read_bytes"
        | "files.include_hidden"
        | "operations.plan_retention_hours" => Ok(()),
        "schema_version" | "vault_id" => Err(KbError::invalid_config(
            key,
            "this identity field cannot be changed through kb config",
        )),
        _ => Err(KbError::invalid_config(key, "unknown configuration key")),
    }
}

fn reject_duplicate_target(document: &Document, key: &str) -> Result<(), KbError> {
    let (section, field) = key
        .split_once('.')
        .ok_or_else(|| KbError::invalid_config(key, "expected a dotted configuration key"))?;
    let root = document
        .as_mapping()
        .ok_or_else(|| KbError::invalid_config("configuration", "root must be a mapping"))?;
    if root.find_all_entries_by_key(section).count() > 1 {
        return Err(KbError::invalid_config(
            key,
            format!("mapping {section} occurs more than once"),
        ));
    }
    if let Some(mapping) = root.get_mapping(section) {
        if mapping.find_all_entries_by_key(field).count() > 1 {
            return Err(KbError::invalid_config(
                key,
                format!("field {field} occurs more than once"),
            ));
        }
    } else if root.contains_key(section) {
        return Err(KbError::invalid_config(
            key,
            format!("{section} must be a mapping"),
        ));
    }
    Ok(())
}

fn set_typed_value(document: &Document, key: &str, value: &str) -> Result<(), KbError> {
    match key {
        "search.mode" => {
            value
                .parse::<kb_core::SearchMode>()
                .map_err(|reason| KbError::invalid_config(key, reason))?;
            document.try_set_path(key, value).map_err(|error| {
                KbError::invalid_config(key, format!("cannot set value: {error}"))
            })?;
        }
        "files.include_hidden" => {
            let parsed = value
                .parse::<bool>()
                .map_err(|_| KbError::invalid_config(key, "expected true or false"))?;
            document.try_set_path(key, parsed).map_err(|error| {
                KbError::invalid_config(key, format!("cannot set value: {error}"))
            })?;
        }
        _ => {
            let parsed = value
                .parse::<u64>()
                .map_err(|_| KbError::invalid_config(key, "expected a positive integer"))?;
            if parsed == 0 {
                return Err(KbError::invalid_config(
                    key,
                    "value must be greater than zero",
                ));
            }
            document.try_set_path(key, parsed).map_err(|error| {
                KbError::invalid_config(key, format!("cannot set value: {error}"))
            })?;
        }
    }
    Ok(())
}
