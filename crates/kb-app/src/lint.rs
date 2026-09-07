use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

use kb_core::{
    CURRENT_SCHEMA_VERSION, EffectiveConfig, KbError, OkfDocumentKind, OkfFinding, OkfSeverity,
    PortableRelativePath, SchemaVersion, detect_portability_collisions, parse_okf, validate_okf,
};
use serde::Serialize;
use time::OffsetDateTime;

use crate::{
    source_io::{Budget, list_files, safe_path},
    source_record,
};

#[derive(Debug, Clone, Serialize)]
pub struct LintReport {
    pub schema_version: SchemaVersion,
    pub checked_files: usize,
    pub findings: Vec<OkfFinding>,
}

/// Inspect authoritative Wiki Markdown without modifying Vault or user state.
///
/// # Errors
///
/// Returns an error when the Vault cannot be traversed safely or configured
/// read limits prevent a complete report.
pub fn lint(
    root: &Path,
    config: &EffectiveConfig,
    now: OffsetDateTime,
) -> Result<LintReport, KbError> {
    let mut budget = Budget::new(config);
    let paths = wiki_markdown_paths(root, &mut budget)?;
    let mut findings = portability_findings(&paths)?;
    let mut documents = BTreeMap::new();

    for path in &paths {
        let bytes = budget.read(&safe_path(root, path.as_str())?)?;
        let text = match String::from_utf8(bytes) {
            Ok(text) => text,
            Err(error) => {
                findings.push(new_finding(
                    path,
                    OkfSeverity::Error,
                    "markdown_not_utf8",
                    Some(1),
                    format!("Markdown is not valid UTF-8: {error}"),
                    "Save the file as UTF-8 Markdown.",
                ));
                continue;
            }
        };
        let document = parse_okf(path.clone(), &text);
        findings.extend(validate_okf(&document, now));
        documents.insert(path.clone(), document);
    }

    let source_versions = collect_source_versions(&documents, &mut findings);
    let mut incoming = BTreeSet::new();
    validate_links(root, &documents, &mut incoming, &mut findings)?;
    validate_supersedes(&documents, &mut incoming, &mut findings);
    validate_source_resources(
        &documents,
        &source_versions,
        &mut incoming,
        &mut findings,
    );
    find_orphans(&documents, &incoming, &mut findings);
    sort_findings(&mut findings);

    Ok(LintReport {
        schema_version: CURRENT_SCHEMA_VERSION,
        checked_files: paths.len(),
        findings,
    })
}

fn wiki_markdown_paths(
    root: &Path,
    budget: &mut Budget,
) -> Result<Vec<PortableRelativePath>, KbError> {
    let mut paths = Vec::new();
    for relative in [
        "Wiki/research",
        "Wiki/articles",
        "Wiki/external-sources/records",
    ] {
        paths.extend(list_files(root, relative, budget)?);
    }
    for relative in ["Wiki/index.md", "Wiki/log.md"] {
        if safe_path(root, relative)?.is_file() {
            budget.visit(&safe_path(root, relative)?)?;
            paths.push(PortableRelativePath::parse(relative)?);
        }
    }
    paths.retain(|path| {
        Path::new(path.as_str())
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("md"))
    });
    paths.sort();
    paths.dedup();
    Ok(paths)
}

fn portability_findings(paths: &[PortableRelativePath]) -> Result<Vec<OkfFinding>, KbError> {
    let native = paths
        .iter()
        .map(PortableRelativePath::to_native_path)
        .collect::<Vec<_>>();
    let mut findings = Vec::new();
    for collision in detect_portability_collisions(&native)? {
        for path in collision.paths {
            findings.push(new_finding(
                &PortableRelativePath::from_path(&path)?,
                OkfSeverity::Error,
                "portable_path_collision",
                None,
                "This path collides by portable case or Unicode rules.",
                "Rename one of the colliding files to a distinct cross-platform path.",
            ));
        }
    }
    Ok(findings)
}

fn collect_source_versions(
    documents: &BTreeMap<PortableRelativePath, kb_core::ParsedOkfDocument>,
    findings: &mut Vec<OkfFinding>,
) -> BTreeMap<String, bool> {
    let mut versions = BTreeMap::new();
    let mut logical_sources = BTreeSet::new();
    for (path, document) in documents {
        if !path
            .as_str()
            .starts_with("Wiki/external-sources/records/")
        {
            continue;
        }
        let Some(frontmatter) = &document.frontmatter else {
            continue;
        };
        let Ok(yaml) = serde_yaml_ng::to_string(frontmatter) else {
            continue;
        };
        let text = format!("---\n{yaml}---\n\n{}", document.body);
        let record = match source_record::parse(&text) {
            Ok(record) => record,
            Err(error) => {
                findings.push(new_finding(
                    path,
                    OkfSeverity::Error,
                    "source_record_invalid",
                    None,
                    format!("Source record cannot be parsed: {}", error.message),
                    "Restore the kb.source structure from a valid source record.",
                ));
                continue;
            }
        };
        let identity_valid = record
            .versions
            .iter()
            .all(|version| version.source == record.source.source)
            && source_record::record_path_for(&record.source.source)
                .is_ok_and(|expected| expected == *path)
            && logical_sources.insert(record.source.source.logical_uri());
        if !identity_valid {
            findings.push(new_finding(
                path,
                OkfSeverity::Error,
                "source_identity_invalid",
                None,
                "Source record identity, history, or storage path is inconsistent.",
                "Restore the record from its captured source identity and version history.",
            ));
            continue;
        }
        let current = record.source.exact_uri();
        for version in record.versions {
            versions.insert(version.exact_uri(), version.exact_uri() == current);
        }
    }
    versions
}

fn validate_links(
    root: &Path,
    documents: &BTreeMap<PortableRelativePath, kb_core::ParsedOkfDocument>,
    incoming: &mut BTreeSet<PortableRelativePath>,
    findings: &mut Vec<OkfFinding>,
) -> Result<(), KbError> {
    for (path, document) in documents {
        for link in &document.links {
            match resolve_wiki_target(path, &link.destination) {
                Target::ExternalOrFragment => {}
                Target::OutsideWiki => findings.push(new_finding(
                    path,
                    OkfSeverity::Error,
                    "link_outside_wiki",
                    Some(link.line),
                    "Markdown link resolves outside Wiki or into .objects.",
                    "Use a Wiki-relative concept path or an absolute external URL.",
                )),
                Target::Invalid => findings.push(new_finding(
                    path,
                    OkfSeverity::Error,
                    "link_target_invalid",
                    Some(link.line),
                    "Markdown link contains invalid percent encoding or a non-portable path.",
                    "Use a UTF-8 portable path and percent-encode reserved characters.",
                )),
                Target::Wiki(mut target) => {
                    let target_path = safe_path(root, target.as_str())?;
                    if target_path.is_dir() {
                        target = PortableRelativePath::parse(&format!(
                            "{}/index.md",
                            target.as_str().trim_end_matches('/')
                        ))?;
                    }
                    if documents.contains_key(&target) {
                        if target != *path {
                            incoming.insert(target);
                        }
                    } else {
                        findings.push(new_finding(
                            path,
                            OkfSeverity::Warning,
                            if document.kind == OkfDocumentKind::Index {
                                "index_drift"
                            } else {
                                "broken_link"
                            },
                            Some(link.line),
                            format!("Markdown target does not exist: {}", target.as_str()),
                            "Create the target or update the link.",
                        ));
                    }
                }
            }
        }
    }
    Ok(())
}

fn validate_supersedes(
    documents: &BTreeMap<PortableRelativePath, kb_core::ParsedOkfDocument>,
    incoming: &mut BTreeSet<PortableRelativePath>,
    findings: &mut Vec<OkfFinding>,
) {
    for (path, document) in documents {
        for value in &document.supersedes {
            let target = PortableRelativePath::parse(&format!("Wiki/{value}"));
            let Ok(target) = target else {
                findings.push(invalid_supersedes(path));
                continue;
            };
            if target == *path {
                findings.push(new_finding(
                    path,
                    OkfSeverity::Error,
                    "supersedes_self",
                    None,
                    "A concept cannot supersede itself.",
                    "Remove the self-reference or name the older concept.",
                ));
                continue;
            }
            let valid = target.as_str().ends_with(".md")
                && !target.as_str().contains("/.objects/")
                && documents
                    .get(&target)
                    .is_some_and(|candidate| candidate.kind == OkfDocumentKind::Concept);
            if valid {
                incoming.insert(target);
            } else {
                findings.push(invalid_supersedes(path));
            }
        }
    }
}

fn invalid_supersedes(path: &PortableRelativePath) -> OkfFinding {
    new_finding(
        path,
        OkfSeverity::Error,
        "supersedes_invalid_target",
        None,
        "kb.supersedes must name an existing Wiki concept.",
        "Use a Wiki-relative .md path that is not index.md, log.md, or .objects.",
    )
}

fn validate_source_resources(
    documents: &BTreeMap<PortableRelativePath, kb_core::ParsedOkfDocument>,
    versions: &BTreeMap<String, bool>,
    incoming: &mut BTreeSet<PortableRelativePath>,
    findings: &mut Vec<OkfFinding>,
) {
    for (path, document) in documents {
        for source in &document.sources {
            if source.resource.starts_with("kb-source://") {
                let exact_shape = source
                    .resource
                    .rsplit_once("?sha256=")
                    .is_some_and(|(logical, digest)| {
                        logical.starts_with("kb-source://")
                            && digest.len() == 64
                            && digest.bytes().all(|byte| {
                                byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)
                            })
                    });
                if !exact_shape || !versions.contains_key(&source.resource) {
                    findings.push(new_finding(
                        path,
                        OkfSeverity::Error,
                        "source_version_missing",
                        source.line,
                        "The exact kb-source version is malformed or absent from source history.",
                        "Capture the source version and reference its exact URI including sha256.",
                    ));
                } else if versions.get(&source.resource) == Some(&false) {
                    findings.push(new_finding(
                        path,
                        OkfSeverity::Warning,
                        "source_version_outdated",
                        source.line,
                        "The referenced source version exists but is not current.",
                        "Review the current source before deciding whether to update this concept.",
                    ));
                }
            } else if let Target::Wiki(target) = resolve_wiki_target(path, &source.resource) {
                if documents.contains_key(&target) && target != *path {
                    incoming.insert(target);
                }
            }
        }
    }
}

fn find_orphans(
    documents: &BTreeMap<PortableRelativePath, kb_core::ParsedOkfDocument>,
    incoming: &BTreeSet<PortableRelativePath>,
    findings: &mut Vec<OkfFinding>,
) {
    for (path, document) in documents {
        if document.kind == OkfDocumentKind::Concept
            && (path.as_str().starts_with("Wiki/research/")
                || path.as_str().starts_with("Wiki/articles/"))
            && !incoming.contains(path)
        {
            findings.push(new_finding(
                path,
                OkfSeverity::Warning,
                "orphan_concept",
                None,
                "No Wiki concept or index links to this concept.",
                "Link it from a relevant concept or index, or leave it intentionally orphaned.",
            ));
        }
    }
}

enum Target {
    ExternalOrFragment,
    OutsideWiki,
    Invalid,
    Wiki(PortableRelativePath),
}

fn resolve_wiki_target(source: &PortableRelativePath, destination: &str) -> Target {
    if destination.starts_with('#') || destination.trim().is_empty() {
        return Target::ExternalOrFragment;
    }
    let without_fragment = destination.split('#').next().unwrap_or(destination);
    let without_query = without_fragment.split('?').next().unwrap_or(without_fragment);
    if without_query.is_empty() {
        return Target::ExternalOrFragment;
    }
    if has_scheme(without_query) {
        return Target::ExternalOrFragment;
    }
    let Some(decoded) = percent_decode(without_query) else {
        return Target::Invalid;
    };
    if decoded.contains('\\') {
        return Target::Invalid;
    }
    let mut components = if decoded.starts_with('/') {
        vec!["Wiki".to_owned()]
    } else {
        source
            .as_str()
            .rsplit_once('/')
            .map_or_else(Vec::new, |(parent, _)| {
                parent.split('/').map(ToOwned::to_owned).collect()
            })
    };
    for component in decoded.trim_start_matches('/').split('/') {
        match component {
            "" | "." => {}
            ".." if components.len() > 1 => {
                components.pop();
            }
            ".." => return Target::OutsideWiki,
            value => components.push(value.to_owned()),
        }
    }
    let joined = components.join("/");
    if !joined.starts_with("Wiki/") || joined.contains("/.objects/") {
        return Target::OutsideWiki;
    }
    match PortableRelativePath::parse(&joined) {
        Ok(path) => Target::Wiki(path),
        Err(_) => Target::Invalid,
    }
}

fn has_scheme(value: &str) -> bool {
    value.find(':').is_some_and(|colon| {
        value.find('/').is_none_or(|slash| colon < slash)
            && value[..colon]
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"+-.".contains(&byte))
    })
}

fn percent_decode(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let high = *bytes.get(index + 1)?;
            let low = *bytes.get(index + 2)?;
            output.push(hex_value(high)? * 16 + hex_value(low)?);
            index += 3;
        } else {
            output.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(output).ok()
}

fn hex_value(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

fn sort_findings(findings: &mut [OkfFinding]) {
    findings.sort_by(|a, b| {
        (&a.path, a.line, a.severity, &a.code).cmp(&(&b.path, b.line, b.severity, &b.code))
    });
}

fn new_finding(
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
