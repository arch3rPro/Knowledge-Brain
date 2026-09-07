use std::collections::{BTreeMap, BTreeSet};

use pulldown_cmark::{Event, Options, Parser, Tag};
use serde::Serialize;
use serde_yaml_ng::{Mapping, Value};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

use crate::PortableRelativePath;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OkfSeverity {
    Error,
    Warning,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OkfDocumentKind {
    Concept,
    Index,
    Log,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkdownLink {
    pub destination: String,
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OkfSourceResource {
    pub id: Option<String>,
    pub resource: String,
    pub line: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OkfFinding {
    pub severity: OkfSeverity,
    pub code: String,
    pub path: PortableRelativePath,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
    pub message: String,
    pub remediation: String,
}

#[derive(Debug, Clone)]
pub struct ParsedOkfDocument {
    pub path: PortableRelativePath,
    pub kind: OkfDocumentKind,
    pub frontmatter: Option<Value>,
    pub body: String,
    pub links: Vec<MarkdownLink>,
    pub sources: Vec<OkfSourceResource>,
    pub supersedes: Vec<String>,
    pub managed: bool,
    field_lines: BTreeMap<String, usize>,
    parse_findings: Vec<OkfFinding>,
}

/// Parse one UTF-8 Wiki Markdown document without resolving cross-file references.
#[must_use]
pub fn parse_okf(path: PortableRelativePath, text: &str) -> ParsedOkfDocument {
    let kind = document_kind(&path);
    let normalized = text.replace("\r\n", "\n");
    let (frontmatter, body, body_line, mut parse_findings) =
        split_frontmatter(&path, kind, &normalized);
    let field_lines = frontmatter_field_lines(&normalized);
    let links = markdown_links(&body, body_line);
    let mapping = frontmatter.as_ref().and_then(Value::as_mapping);
    let managed = mapping
        .and_then(|value| mapping_value(value, "kb"))
        .and_then(Value::as_mapping)
        .and_then(|value| mapping_value(value, "managed"))
        .and_then(Value::as_bool)
        == Some(true);
    let sources = mapping.map_or_else(Vec::new, source_resources);
    let (supersedes, invalid_supersedes) = mapping.map(supersedes).unwrap_or_default();
    if invalid_supersedes {
        parse_findings.push(finding(
            &path,
            OkfSeverity::Error,
            "supersedes_entry_invalid",
            field_line(&normalized, "supersedes"),
            "kb.supersedes entries must be strings.",
            "Use a list of Wiki-relative Markdown concept paths.",
        ));
    }
    ParsedOkfDocument {
        path,
        kind,
        frontmatter,
        body,
        links,
        sources,
        supersedes,
        managed,
        field_lines,
        parse_findings,
    }
}

/// Validate OKF v0.2 and the explicit Knowledge-Brain Producer Profile.
#[must_use]
pub fn validate_okf(document: &ParsedOkfDocument, now: OffsetDateTime) -> Vec<OkfFinding> {
    let mut findings = document.parse_findings.clone();
    match document.kind {
        OkfDocumentKind::Concept => validate_concept(document, now, &mut findings),
        OkfDocumentKind::Index => validate_index(document, &mut findings),
        OkfDocumentKind::Log => validate_log(document, &mut findings),
    }
    findings.sort_by(|a, b| {
        (&a.path, a.line, a.severity, &a.code).cmp(&(&b.path, b.line, b.severity, &b.code))
    });
    findings
}

fn document_kind(path: &PortableRelativePath) -> OkfDocumentKind {
    match path.as_str().rsplit('/').next() {
        Some("index.md") => OkfDocumentKind::Index,
        Some("log.md") => OkfDocumentKind::Log,
        _ => OkfDocumentKind::Concept,
    }
}

fn split_frontmatter(
    path: &PortableRelativePath,
    kind: OkfDocumentKind,
    text: &str,
) -> (Option<Value>, String, usize, Vec<OkfFinding>) {
    if !text.starts_with("---\n") {
        let findings = if kind == OkfDocumentKind::Concept {
            vec![finding(
                path,
                OkfSeverity::Error,
                "frontmatter_required",
                Some(1),
                "Concept documents require YAML frontmatter at the start of the file.",
                "Add a frontmatter block containing a non-empty type.",
            )]
        } else {
            Vec::new()
        };
        return (None, text.to_owned(), 1, findings);
    }
    let after_open = &text[4..];
    let (yaml_text, body, body_line) = if let Some(end) = after_open.find("\n---\n") {
        let body_start = 4 + end + 5;
        (
            &after_open[..end],
            text[body_start..].to_owned(),
            text[..body_start]
                .bytes()
                .filter(|byte| *byte == b'\n')
                .count()
                + 1,
        )
    } else if let Some(yaml_text) = after_open.strip_suffix("\n---") {
        (
            yaml_text,
            String::new(),
            text.bytes().filter(|byte| *byte == b'\n').count() + 1,
        )
    } else {
        return (
            None,
            String::new(),
            1,
            vec![finding(
                path,
                OkfSeverity::Error,
                "frontmatter_unclosed",
                Some(1),
                "The YAML frontmatter block is not closed.",
                "Close the block with --- on its own line.",
            )],
        );
    };
    let parsed = serde_yaml_ng::from_str::<Value>(yaml_text);
    let value = match parsed {
        Ok(value) if value.is_mapping() => Some(value),
        Ok(_) => {
            return (
                None,
                body,
                body_line,
                vec![finding(
                    path,
                    OkfSeverity::Error,
                    "frontmatter_invalid",
                    Some(2),
                    "Frontmatter must be a YAML mapping.",
                    "Use named YAML fields such as type: Article.",
                )],
            );
        }
        Err(error) => {
            return (
                None,
                body,
                body_line,
                vec![finding(
                    path,
                    OkfSeverity::Error,
                    "frontmatter_invalid",
                    error.location().map(|location| location.line() + 1),
                    format!("Frontmatter is not valid YAML: {error}"),
                    "Correct the YAML syntax without removing unknown fields.",
                )],
            );
        }
    };
    let findings = if kind != OkfDocumentKind::Concept
        && !(kind == OkfDocumentKind::Index && path.as_str() == "Wiki/index.md")
    {
        vec![finding(
            path,
            OkfSeverity::Error,
            "reserved_frontmatter_forbidden",
            Some(1),
            "This reserved OKF file must not contain frontmatter.",
            "Remove frontmatter and keep only the Markdown body.",
        )]
    } else {
        Vec::new()
    };
    (value, body, body_line, findings)
}

fn validate_concept(
    document: &ParsedOkfDocument,
    now: OffsetDateTime,
    findings: &mut Vec<OkfFinding>,
) {
    let Some(mapping) = document.frontmatter.as_ref().and_then(Value::as_mapping) else {
        return;
    };
    require_nonempty_string(
        document,
        mapping,
        "type",
        "type_required",
        "Concept frontmatter requires a non-empty string type.",
        findings,
    );
    validate_status(document, mapping, findings);
    validate_generated(document, mapping, findings);
    validate_verified(document, mapping, findings);
    validate_sources(document, mapping, findings);
    if let Some(window) = mapping_value(mapping, "usage_window") {
        validate_usage_window(document, "usage_window", window, findings);
    }
    validate_stale_after(document, mapping, now, findings);
    if document.managed {
        validate_managed(document, mapping, findings);
    }
}

fn validate_index(document: &ParsedOkfDocument, findings: &mut Vec<OkfFinding>) {
    if document.path.as_str() != "Wiki/index.md" {
        return;
    }
    if let Some(mapping) = document.frontmatter.as_ref().and_then(Value::as_mapping) {
        if mapping_value(mapping, "okf_version").and_then(Value::as_str) != Some("0.2") {
            findings.push(finding(
                &document.path,
                OkfSeverity::Error,
                "okf_version_unsupported",
                None,
                "Wiki/index.md declares an unsupported OKF version.",
                "Set okf_version to \"0.2\" or omit the frontmatter.",
            ));
        }
    }
}

fn validate_log(document: &ParsedOkfDocument, findings: &mut Vec<OkfFinding>) {
    for (index, line) in document.body.lines().enumerate() {
        if let Some(heading) = line.strip_prefix("## ") {
            let valid = heading.len() == 10
                && heading.as_bytes()[4] == b'-'
                && heading.as_bytes()[7] == b'-'
                && heading
                    .bytes()
                    .enumerate()
                    .all(|(i, byte)| i == 4 || i == 7 || byte.is_ascii_digit());
            if !valid {
                findings.push(finding(
                    &document.path,
                    OkfSeverity::Error,
                    "log_date_heading_invalid",
                    Some(index + 1),
                    "Log level-two headings must use YYYY-MM-DD.",
                    "Replace the heading with an ISO 8601 calendar date.",
                ));
            }
        }
    }
}

fn validate_status(
    document: &ParsedOkfDocument,
    mapping: &Mapping,
    findings: &mut Vec<OkfFinding>,
) {
    let Some(status) = mapping_value(mapping, "status") else {
        return;
    };
    if !matches!(status.as_str(), Some("draft" | "stable" | "deprecated")) {
        findings.push(field_finding(
            document,
            "status",
            "status_invalid",
            "status must be draft, stable, or deprecated.",
            "Use one of the OKF v0.2 lifecycle values.",
        ));
    }
}

fn validate_generated(
    document: &ParsedOkfDocument,
    mapping: &Mapping,
    findings: &mut Vec<OkfFinding>,
) {
    let Some(generated) = mapping_value(mapping, "generated") else {
        return;
    };
    let Some(value) = generated.as_mapping() else {
        findings.push(field_finding(
            document,
            "generated",
            "generated_invalid",
            "generated must be a mapping.",
            "Use generated: { by: actor, at: timestamp }.",
        ));
        return;
    };
    if !is_nonempty_string(mapping_value(value, "by")) {
        findings.push(field_finding(
            document,
            "generated",
            "generated_actor_required",
            "generated.by must be a non-empty actor string.",
            "Identify the agent, human, or process that produced the content.",
        ));
    } else if !mapping_value(value, "by")
        .and_then(Value::as_str)
        .is_some_and(valid_actor)
    {
        findings.push(field_finding(
            document,
            "generated",
            "generated_actor_invalid",
            "generated.by does not follow the OKF actor convention.",
            "Use producer/version, human:<id>, or process:<id>.",
        ));
    }
    if let Some(at) = mapping_value(value, "at") {
        if parse_timestamp(at).is_none() {
            findings.push(field_finding(
                document,
                "generated",
                "generated_timestamp_invalid",
                "generated.at must be an ISO 8601 timestamp with an explicit UTC offset.",
                "Use a timestamp such as 2026-09-07T03:00:00Z.",
            ));
        }
    }
}

fn validate_verified(
    document: &ParsedOkfDocument,
    mapping: &Mapping,
    findings: &mut Vec<OkfFinding>,
) {
    let Some(verified) = mapping_value(mapping, "verified") else {
        return;
    };
    let events: Vec<&Value> = if verified.is_mapping() {
        vec![verified]
    } else if let Some(sequence) = verified.as_sequence() {
        sequence.iter().collect()
    } else {
        findings.push(field_finding(
            document,
            "verified",
            "verified_invalid",
            "verified must be one mapping or a list of mappings.",
            "Record each verification as { by: actor, at: timestamp }.",
        ));
        return;
    };
    for event in events {
        let Some(event) = event.as_mapping() else {
            findings.push(field_finding(
                document,
                "verified",
                "verified_invalid",
                "Every verified event must be a mapping.",
                "Record each verification as { by: actor, at: timestamp }.",
            ));
            continue;
        };
        if !is_nonempty_string(mapping_value(event, "by")) {
            findings.push(field_finding(
                document,
                "verified",
                "verified_actor_required",
                "verified.by must be a non-empty actor string.",
                "Identify the human, agent, or process that verified the content.",
            ));
        } else if !mapping_value(event, "by")
            .and_then(Value::as_str)
            .is_some_and(valid_actor)
        {
            findings.push(field_finding(
                document,
                "verified",
                "verified_actor_invalid",
                "verified.by does not follow the OKF actor convention.",
                "Use producer/version, human:<id>, or process:<id>.",
            ));
        }
        if mapping_value(event, "at")
            .and_then(parse_timestamp)
            .is_none()
        {
            findings.push(field_finding(
                document,
                "verified",
                "verified_timestamp_invalid",
                "verified.at must be an ISO 8601 timestamp with an explicit UTC offset.",
                "Use a timestamp such as 2026-09-07T03:00:00Z.",
            ));
        }
    }
}

fn validate_sources(
    document: &ParsedOkfDocument,
    mapping: &Mapping,
    findings: &mut Vec<OkfFinding>,
) {
    let Some(sources) = mapping_value(mapping, "sources") else {
        return;
    };
    let Some(sequence) = sources.as_sequence() else {
        findings.push(field_finding(
            document,
            "sources",
            "sources_invalid",
            "sources must be a YAML list.",
            "Use one mapping per source.",
        ));
        return;
    };
    let mut ids = BTreeSet::new();
    for source in sequence {
        let Some(source) = source.as_mapping() else {
            findings.push(field_finding(
                document,
                "sources",
                "source_invalid",
                "Every source must be a mapping.",
                "Use resource and optional id fields for each source.",
            ));
            continue;
        };
        if !is_nonempty_string(mapping_value(source, "resource")) {
            findings.push(field_finding(
                document,
                "sources",
                "source_resource_required",
                "Every source requires a non-empty resource string.",
                "Set resource to a URL, bundle path, exact kb-source URI, or scope descriptor.",
            ));
        }
        if let Some(id) = mapping_value(source, "id").and_then(Value::as_str) {
            if !id.trim().is_empty() && !ids.insert(id) {
                findings.push(field_finding(
                    document,
                    "sources",
                    "source_id_duplicate",
                    "Source IDs must be unique within a concept.",
                    "Give each source a stable unique id.",
                ));
            }
        }
        if mapping_value(source, "last_modified")
            .is_some_and(|value| parse_timestamp(value).is_none())
        {
            findings.push(field_finding(
                document,
                "sources",
                "source_last_modified_invalid",
                "sources[].last_modified must be an ISO 8601 timestamp with an explicit UTC offset.",
                "Use an absolute timestamp such as 2026-09-07T03:00:00Z.",
            ));
        }
        if let Some(window) = mapping_value(source, "usage_window") {
            validate_usage_window(document, "sources", window, findings);
        }
    }
}

fn validate_usage_window(
    document: &ParsedOkfDocument,
    field: &str,
    value: &Value,
    findings: &mut Vec<OkfFinding>,
) {
    let valid = value.as_mapping().is_some_and(|window| {
        mapping_value(window, "from")
            .and_then(parse_timestamp)
            .is_some()
            && mapping_value(window, "to")
                .and_then(parse_timestamp)
                .is_some()
    });
    if !valid {
        findings.push(field_finding(
            document,
            field,
            "usage_window_invalid",
            "usage_window must contain valid from and to timestamps with explicit UTC offsets.",
            "Set both from and to to absolute ISO 8601 timestamps.",
        ));
    }
}

fn validate_stale_after(
    document: &ParsedOkfDocument,
    mapping: &Mapping,
    now: OffsetDateTime,
    findings: &mut Vec<OkfFinding>,
) {
    let Some(value) = mapping_value(mapping, "stale_after") else {
        return;
    };
    let Some(stale_after) = parse_timestamp(value) else {
        findings.push(field_finding(
            document,
            "stale_after",
            "stale_after_invalid",
            "stale_after must be an ISO 8601 timestamp with an explicit UTC offset.",
            "Use an absolute timestamp such as 2026-09-07T03:00:00Z.",
        ));
        return;
    };
    if now >= stale_after {
        findings.push(field_finding_with_severity(
            document,
            "stale_after",
            OkfSeverity::Warning,
            "stale_document",
            "The concept is at or past stale_after.",
            "Review the concept against its sources and update its lifecycle metadata.",
        ));
    }
}

fn validate_managed(
    document: &ParsedOkfDocument,
    mapping: &Mapping,
    findings: &mut Vec<OkfFinding>,
) {
    require_nonempty_string(
        document,
        mapping,
        "title",
        "managed_title_required",
        "Managed concepts require a non-empty title.",
        findings,
    );
    if mapping_value(mapping, "status").is_none() {
        findings.push(field_finding(
            document,
            "status",
            "managed_status_required",
            "Managed concepts require an explicit status.",
            "Set status to draft, stable, or deprecated.",
        ));
    }
    let generated = mapping_value(mapping, "generated").and_then(Value::as_mapping);
    if generated.is_none() {
        findings.push(field_finding(
            document,
            "generated",
            "managed_generated_required",
            "Managed concepts require generated metadata.",
            "Set generated.by and generated.at.",
        ));
    } else if generated
        .and_then(|value| mapping_value(value, "at"))
        .and_then(parse_timestamp)
        .is_none()
    {
        findings.push(field_finding(
            document,
            "generated",
            "managed_generated_timestamp_required",
            "Managed concepts require a valid generated.at timestamp.",
            "Use an ISO 8601 timestamp with an explicit UTC offset.",
        ));
    }
    let sources = mapping_value(mapping, "sources").and_then(Value::as_sequence);
    if sources.is_none_or(Vec::is_empty) {
        findings.push(field_finding(
            document,
            "sources",
            "managed_sources_required",
            "Managed concepts require at least one source.",
            "Add a source with a stable id and resource.",
        ));
    } else if let Some(sources) = sources {
        for source in sources {
            if !source
                .as_mapping()
                .and_then(|value| mapping_value(value, "id"))
                .is_some_and(is_nonempty_string_value)
            {
                findings.push(field_finding(
                    document,
                    "sources",
                    "managed_source_id_required",
                    "Every managed source requires a non-empty id.",
                    "Add a stable id used by claim footnotes and updates.",
                ));
            }
        }
    }
}

fn markdown_links(body: &str, first_line: usize) -> Vec<MarkdownLink> {
    Parser::new_ext(body, Options::all())
        .into_offset_iter()
        .filter_map(|(event, range)| match event {
            Event::Start(Tag::Link { dest_url, .. }) => Some(MarkdownLink {
                destination: dest_url.into_string(),
                line: first_line
                    + body[..range.start]
                        .bytes()
                        .filter(|byte| *byte == b'\n')
                        .count(),
            }),
            _ => None,
        })
        .collect()
}

fn source_resources(mapping: &Mapping) -> Vec<OkfSourceResource> {
    mapping_value(mapping, "sources")
        .and_then(Value::as_sequence)
        .into_iter()
        .flatten()
        .filter_map(Value::as_mapping)
        .filter_map(|source| {
            let resource = mapping_value(source, "resource")?.as_str()?.to_owned();
            Some(OkfSourceResource {
                id: mapping_value(source, "id")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned),
                resource,
                line: None,
            })
        })
        .collect()
}

fn supersedes(mapping: &Mapping) -> (Vec<String>, bool) {
    let Some(values) = mapping_value(mapping, "kb")
        .and_then(Value::as_mapping)
        .and_then(|value| mapping_value(value, "supersedes"))
    else {
        return (Vec::new(), false);
    };
    let Some(values) = values.as_sequence() else {
        return (Vec::new(), true);
    };
    let mut invalid = false;
    let strings = values
        .iter()
        .filter_map(|value| {
            if let Some(value) = value.as_str() {
                Some(value.to_owned())
            } else {
                invalid = true;
                None
            }
        })
        .collect();
    (strings, invalid)
}

fn require_nonempty_string(
    document: &ParsedOkfDocument,
    mapping: &Mapping,
    field: &str,
    code: &str,
    message: &str,
    findings: &mut Vec<OkfFinding>,
) {
    if !is_nonempty_string(mapping_value(mapping, field)) {
        findings.push(field_finding(
            document,
            field,
            code,
            message,
            &format!("Set {field} to a non-empty string."),
        ));
    }
}

fn mapping_value<'a>(mapping: &'a Mapping, key: &str) -> Option<&'a Value> {
    mapping.get(Value::String(key.to_owned()))
}

fn is_nonempty_string(value: Option<&Value>) -> bool {
    value.is_some_and(is_nonempty_string_value)
}

fn is_nonempty_string_value(value: &Value) -> bool {
    value.as_str().is_some_and(|value| !value.trim().is_empty())
}

fn parse_timestamp(value: &Value) -> Option<OffsetDateTime> {
    OffsetDateTime::parse(value.as_str()?, &Rfc3339).ok()
}

fn valid_actor(value: &str) -> bool {
    if let Some(id) = value.strip_prefix("human:") {
        return !id.trim().is_empty();
    }
    if let Some(id) = value.strip_prefix("process:") {
        return !id.trim().is_empty();
    }
    value.split_once('/').is_some_and(|(producer, version)| {
        !producer.trim().is_empty() && !version.trim().is_empty()
    })
}

fn field_line(text: &str, field: &str) -> Option<usize> {
    text.lines().enumerate().find_map(|(index, line)| {
        (line.trim_start().starts_with(&format!("{field}:"))).then_some(index + 1)
    })
}

fn field_finding(
    document: &ParsedOkfDocument,
    field: &str,
    code: &str,
    message: &str,
    remediation: &str,
) -> OkfFinding {
    field_finding_with_severity(
        document,
        field,
        OkfSeverity::Error,
        code,
        message,
        remediation,
    )
}

fn field_finding_with_severity(
    document: &ParsedOkfDocument,
    field: &str,
    severity: OkfSeverity,
    code: &str,
    message: &str,
    remediation: &str,
) -> OkfFinding {
    finding(
        &document.path,
        severity,
        code,
        document
            .frontmatter
            .as_ref()
            .and_then(|_| field_line_from_document(document, field)),
        message,
        remediation,
    )
}

fn field_line_from_document(document: &ParsedOkfDocument, field: &str) -> Option<usize> {
    document.field_lines.get(field).copied()
}

fn frontmatter_field_lines(text: &str) -> BTreeMap<String, usize> {
    if !text.starts_with("---\n") {
        return BTreeMap::new();
    }
    let mut lines = BTreeMap::new();
    for (index, line) in text.lines().enumerate().skip(1) {
        if line == "---" {
            break;
        }
        let trimmed = line.trim_start();
        if line != trimmed || trimmed.starts_with('#') {
            continue;
        }
        if let Some((key, _)) = trimmed.split_once(':') {
            if !key.is_empty() && !key.chars().any(char::is_whitespace) {
                lines.entry(key.to_owned()).or_insert(index + 1);
            }
        }
    }
    lines
}

fn finding(
    path: &PortableRelativePath,
    severity: OkfSeverity,
    code: &str,
    line: Option<usize>,
    message: impl Into<String>,
    remediation: impl Into<String>,
) -> OkfFinding {
    OkfFinding {
        severity,
        code: code.to_owned(),
        path: path.clone(),
        line,
        message: message.into(),
        remediation: remediation.into(),
    }
}
