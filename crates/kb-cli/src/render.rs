use std::{fmt::Write as _, io::Write, process::ExitCode};

use kb_core::KbError;
use kb_protocol::{Envelope, ErrorEnvelope};
use serde_json::Value;

pub(crate) fn success(
    value: &Value,
    json_output: bool,
    fail_on_findings: bool,
    full_hashes: bool,
) -> ExitCode {
    let rendered = if json_output {
        serde_json::to_string(&Envelope::new(value))
            .map_err(|error| KbError::invalid_config("command response", error.to_string()))
    } else {
        human(value, full_hashes)
    };
    match rendered {
        Ok(text) => {
            println!("{text}");
            if fail_on_findings
                && value
                    .get("findings")
                    .and_then(Value::as_array)
                    .is_some_and(|findings| !findings.is_empty())
            {
                ExitCode::FAILURE
            } else {
                ExitCode::SUCCESS
            }
        }
        Err(render_error) => error(render_error, json_output),
    }
}

#[allow(clippy::unnecessary_wraps)]
pub(crate) fn human(value: &Value, full_hashes: bool) -> Result<String, KbError> {
    if value.get("kind").and_then(Value::as_str) == Some("maintenance") {
        return Ok(render_maintenance(value));
    }
    if let Some(diff) = value.get("diff").and_then(Value::as_str) {
        return Ok(diff.to_owned());
    }
    if let Some(summary) = value.get("operation_summary").and_then(Value::as_object) {
        return Ok(render_operation_summary(summary, full_hashes));
    }
    if let Some(summary) = value.get("change_summary").and_then(Value::as_object) {
        return Ok(render_change_summary(value, summary, full_hashes));
    }
    if let Some(groups) = value.get("groups").and_then(Value::as_array) {
        return Ok(render_query_groups(value, groups, full_hashes));
    }
    if let Some(checks) = doctor_checks(value) {
        return Ok(render_doctor_checks(checks, full_hashes));
    }

    let mut output = String::new();
    render_value(&mut output, value, 0, full_hashes, false);
    Ok(output.trim_end().to_owned())
}

#[allow(clippy::too_many_lines)]
fn render_maintenance(value: &Value) -> String {
    let mut lines = Vec::new();
    if let Some(root) = value.get("root").and_then(Value::as_str) {
        lines.push(format!("vault: {root}"));
    }

    if let Some(status) = value.get("status") {
        let pending = status
            .pointer("/recovery/pending_operations")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        if pending > 0 {
            lines.push(format!("recovery: {pending} pending operation(s)"));
        }
        if let Some(compatibility) = status
            .pointer("/schema/compatibility")
            .and_then(Value::as_str)
            .filter(|compatibility| *compatibility != "current")
        {
            lines.push(format!("schema: {compatibility}"));
        }
        if let Some(configuration) = status
            .get("configuration")
            .and_then(Value::as_str)
            .filter(|configuration| *configuration != "valid")
        {
            lines.push(format!("configuration: {configuration}"));
        }
    }

    if let Some(sources) = value.get("sources") {
        if sources.get("status").and_then(Value::as_str) == Some("not_checked") {
            lines.push("sources: not checked (Vault recovery is required)".to_owned());
        } else {
            let changes = sources
                .get("changes")
                .and_then(Value::as_array)
                .map_or(0, Vec::len);
            let unchanged = sources
                .get("unchanged")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            let skipped = sources
                .get("skipped")
                .and_then(Value::as_array)
                .map_or(0, Vec::len);
            if changes == 0 {
                lines.push(format!("sources: no changes ({unchanged} unchanged)"));
            } else {
                let mut counts = std::collections::BTreeMap::new();
                for change in sources
                    .get("changes")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    if let Some(kind) = change.get("kind").and_then(Value::as_str) {
                        *counts.entry(kind).or_insert(0usize) += 1;
                    }
                }
                let breakdown = counts
                    .into_iter()
                    .map(|(kind, count)| format!("{kind} {count}"))
                    .collect::<Vec<_>>()
                    .join(", ");
                lines.push(format!("sources: {changes} change(s) ({breakdown})"));
            }
            if skipped > 0 {
                lines.push(format!("sources_skipped: {skipped}"));
            }
        }
    }

    if let Some(lint) = value.get("lint") {
        if lint.get("status").and_then(Value::as_str) == Some("not_checked") {
            lines.push("lint: not checked (Vault recovery is required)".to_owned());
        } else {
            let checked = lint
                .get("checked_files")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            let findings = lint
                .get("findings")
                .and_then(Value::as_array)
                .map_or(0, Vec::len);
            lines.push(if findings == 0 {
                format!("lint: no findings in {checked} file(s)")
            } else {
                format!("lint: {findings} finding(s) in {checked} file(s)")
            });
        }
    }

    if let Some(checks) = value
        .get("doctor")
        .and_then(|doctor| doctor.get("checks"))
        .and_then(Value::as_array)
    {
        let mut counts = std::collections::BTreeMap::new();
        for check in checks {
            if let Some(status) = check.get("status").and_then(Value::as_str) {
                *counts.entry(status).or_insert(0usize) += 1;
            }
        }
        let summary = counts
            .into_iter()
            .map(|(status, count)| format!("{status} {count}"))
            .collect::<Vec<_>>()
            .join(", ");
        lines.push(format!("doctor: {summary}"));
    }
    lines.join("\n")
}

fn render_operation_summary(summary: &serde_json::Map<String, Value>, full_hashes: bool) -> String {
    const SCALAR_FIELDS: [&str; 9] = [
        "summary",
        "operation_id",
        "operation_kind",
        "operation_state",
        "vault_id",
        "vault_root",
        "change_count",
        "requires_confirmation",
        "can_apply",
    ];
    let mut output = String::new();
    for field in SCALAR_FIELDS {
        if let Some(value) = summary.get(field) {
            push_scalar_line(
                &mut output,
                0,
                Some(field),
                value,
                full_hashes || identity_field(field),
            );
        }
    }
    if let Some(paths) = summary.get("affected_paths").and_then(Value::as_array) {
        output.push_str("affected_paths:\n");
        for path in paths {
            push_scalar_line(&mut output, 2, None, path, true);
        }
    }
    output.trim_end().to_owned()
}

fn render_change_summary(
    value: &Value,
    summary: &serde_json::Map<String, Value>,
    full_hashes: bool,
) -> String {
    let mut output = String::new();
    if let Some(phase) = value.get("phase") {
        push_scalar_line(&mut output, 0, Some("phase"), phase, full_hashes);
    }
    for field in ["summary", "operation_kind", "change_count"] {
        if let Some(value) = summary.get(field) {
            push_scalar_line(&mut output, 0, Some(field), value, full_hashes);
        }
    }
    if let Some(paths) = summary.get("affected_paths").and_then(Value::as_array) {
        output.push_str("affected_paths:\n");
        for path in paths {
            push_scalar_line(&mut output, 2, None, path, true);
        }
    }
    output.trim_end().to_owned()
}

fn render_query_groups(value: &Value, groups: &[Value], full_hashes: bool) -> String {
    let mut output = String::new();
    let mut has_results = false;
    if let Some(match_mode) = value.get("match_mode") {
        push_scalar_line(&mut output, 0, Some("match_mode"), match_mode, full_hashes);
    }
    for group in groups {
        let Some(group) = group.as_object() else {
            render_value(&mut output, group, 0, full_hashes, false);
            continue;
        };
        let scope = group
            .get("scope")
            .and_then(Value::as_str)
            .unwrap_or("results");
        let Some(results) = group.get("results").and_then(Value::as_array) else {
            render_value(
                &mut output,
                &Value::Object(group.clone()),
                0,
                full_hashes,
                false,
            );
            continue;
        };
        for result in results {
            has_results = true;
            let Some(result) = result.as_object() else {
                render_value(&mut output, result, 0, full_hashes, false);
                continue;
            };
            let _ = writeln!(output, "scope: {scope}");
            for field in ["path", "title", "snippet"] {
                if let Some(value) = result.get(field) {
                    push_scalar_line(
                        &mut output,
                        0,
                        Some(field),
                        value,
                        full_hashes || identity_field(field),
                    );
                }
            }
        }
    }
    if !has_results {
        output.push_str("No results.\n");
    }
    if let Some(warnings) = value.get("warnings").and_then(Value::as_array) {
        for warning in warnings {
            push_scalar_line(&mut output, 0, Some("warning"), warning, full_hashes);
        }
    }
    output.trim_end().to_owned()
}

fn doctor_checks(value: &Value) -> Option<&[Value]> {
    value.get("root")?.as_str()?;
    let checks = value.get("checks")?.as_array()?;
    checks
        .iter()
        .all(|check| {
            check.get("id").and_then(Value::as_str).is_some()
                && check.get("status").and_then(Value::as_str).is_some()
                && check.get("message").and_then(Value::as_str).is_some()
        })
        .then_some(checks)
}

fn render_doctor_checks(checks: &[Value], full_hashes: bool) -> String {
    let mut output = String::new();
    for check in checks {
        let status = check["status"].as_str().unwrap_or("unknown");
        let id = check["id"].as_str().unwrap_or("unknown");
        let message = check["message"].as_str().unwrap_or_default();
        output.push_str(status);
        output.push(' ');
        output.push_str(id);
        output.push_str(": ");
        output.push_str(display_hash(message, full_hashes));
        output.push('\n');
    }
    output.trim_end().to_owned()
}

fn render_value(
    output: &mut String,
    value: &Value,
    indent: usize,
    full_hashes: bool,
    preserve_identity: bool,
) {
    match value {
        Value::Object(object) => {
            if object.is_empty() {
                push_text_line(output, indent, "(empty)");
                return;
            }
            for (key, value) in object {
                let preserve_identity = preserve_identity || identity_field(key);
                if is_scalar(value) {
                    push_scalar_line(
                        output,
                        indent,
                        Some(key),
                        value,
                        full_hashes || preserve_identity,
                    );
                } else {
                    push_text_line(output, indent, &format!("{key}:"));
                    render_value(output, value, indent + 2, full_hashes, preserve_identity);
                }
            }
        }
        Value::Array(values) => {
            if values.is_empty() {
                push_text_line(output, indent, "(none)");
                return;
            }
            for value in values {
                if is_scalar(value) {
                    push_scalar_line(
                        output,
                        indent,
                        None,
                        value,
                        full_hashes || preserve_identity,
                    );
                } else {
                    push_text_line(output, indent, "-");
                    render_value(output, value, indent + 2, full_hashes, preserve_identity);
                }
            }
        }
        _ => push_scalar_line(
            output,
            indent,
            None,
            value,
            full_hashes || preserve_identity,
        ),
    }
}

fn identity_field(key: &str) -> bool {
    matches!(
        key,
        "id" | "path" | "paths" | "root" | "target" | "archive" | "source_uri"
    ) || key.ends_with("_id")
        || key.ends_with("_ids")
        || key.ends_with("_path")
        || key.ends_with("_paths")
        || key.ends_with("_root")
        || key.ends_with("_uri")
}

fn push_scalar_line(
    output: &mut String,
    indent: usize,
    key: Option<&str>,
    value: &Value,
    full_hashes: bool,
) {
    let prefix = key.map_or("- ".to_owned(), |key| format!("{key}: "));
    let rendered = scalar(value, full_hashes);
    let mut lines = rendered.lines();
    push_text_line(
        output,
        indent,
        &format!("{prefix}{}", lines.next().unwrap_or_default()),
    );
    for line in lines {
        push_text_line(output, indent + 2, line);
    }
}

fn push_text_line(output: &mut String, indent: usize, text: &str) {
    output.push_str(&" ".repeat(indent));
    output.push_str(text);
    output.push('\n');
}

fn is_scalar(value: &Value) -> bool {
    matches!(
        value,
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_)
    )
}

fn scalar(value: &Value, full_hashes: bool) -> String {
    match value {
        Value::Null => "null".to_owned(),
        Value::Bool(value) => value.to_string(),
        Value::Number(value) => value.to_string(),
        Value::String(value) => display_hash(value, full_hashes).to_owned(),
        Value::Array(_) | Value::Object(_) => unreachable!("non-scalar value"),
    }
}

fn display_hash(value: &str, full_hashes: bool) -> &str {
    if !full_hashes && value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        &value[..12]
    } else {
        value
    }
}

pub(crate) fn error(error: KbError, json_output: bool) -> ExitCode {
    if json_output {
        match serde_json::to_string(&ErrorEnvelope::from(error)) {
            Ok(text) => println!("{text}"),
            Err(serialization_error) => eprintln!("Error: {serialization_error}"),
        }
    } else {
        eprintln!("Error: {error}");
    }
    ExitCode::FAILURE
}

pub(crate) fn startup(value: &Value) -> Result<(), KbError> {
    let text = serde_json::to_string(&Envelope::new(value))
        .map_err(|error| KbError::invalid_config("server startup response", error.to_string()))?;
    println!("{text}");
    std::io::stdout().flush().map_err(|error| {
        KbError::io_failure("flush server startup response", "stdout", error.to_string())
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::human;

    #[test]
    fn identity_fields_are_never_shortened_even_when_they_look_like_hashes() {
        let path = "a".repeat(64);
        let operation_id = "b".repeat(64);
        let affected_path = "c".repeat(64);
        let sha256 = "d".repeat(64);

        let rendered = human(
            &json!({
                "path": path,
                "operation_id": operation_id,
                "affected_paths": [affected_path],
                "sha256": sha256,
            }),
            false,
        )
        .unwrap();

        assert!(rendered.contains(&format!("path: {path}")), "{rendered}");
        assert!(
            rendered.contains(&format!("operation_id: {operation_id}")),
            "{rendered}"
        );
        assert!(
            rendered.contains(&format!("- {affected_path}")),
            "{rendered}"
        );
        assert!(rendered.contains("sha256: dddddddddddd"), "{rendered}");
        assert!(
            !rendered.contains(&format!("sha256: {sha256}")),
            "{rendered}"
        );
    }
}
