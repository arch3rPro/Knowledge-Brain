use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

use kb_core::{
    CURRENT_SCHEMA_VERSION, ErrorCode, KbError, OkfSeverity, PortableRelativePath, SchemaVersion,
    SearchMode, detect_portability_collisions,
};
use serde::Serialize;
use uuid::Uuid;

use crate::{ConfigOverrides, UserPaths, lint, load_effective_config, vault_status};

const MAX_GIT_OUTPUT: usize = 1024 * 1024;
const GITIGNORE_LINES: [&str; 10] = [
    "/.kb/config.local.yml",
    "/.kb/cache/",
    "/.kb/runtime/",
    "/.obsidian/workspace.json",
    "/.obsidian/workspace-mobile.json",
    "/.obsidian/workspaces.json",
    "/.trash/",
    ".DS_Store",
    "Thumbs.db",
    "Desktop.ini",
];
const GITATTRIBUTES_LINES: [&str; 5] = [
    "*.md text eol=lf",
    "*.yml text eol=lf",
    "*.yaml text eol=lf",
    "*.json text eol=lf",
    "/Wiki/external-sources/.objects/** -text",
];
const LOCAL_PATHS: [&str; 9] = [
    ".kb/config.local.yml",
    ".kb/cache",
    ".kb/runtime",
    ".obsidian/workspace.json",
    ".obsidian/workspace-mobile.json",
    ".obsidian/workspaces.json",
    ".trash",
    ".DS_Store",
    "Thumbs.db",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncFindingLevel {
    Error,
    Warning,
    Info,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SyncFinding {
    pub code: String,
    pub level: SyncFindingLevel,
    pub blocks_writes: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    pub message: String,
    pub next_action: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GitInspectionState {
    Checked,
    NotRepository,
    Unavailable,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GitInspection {
    pub state: GitInspectionState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repository_root: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SyncSummary {
    pub errors: usize,
    pub warnings: usize,
    pub information: usize,
    pub writes_blocked: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct SyncReport {
    pub schema_version: SchemaVersion,
    pub vault_id: Uuid,
    pub root: PathBuf,
    pub summary: SyncSummary,
    pub git: GitInspection,
    pub findings: Vec<SyncFinding>,
}

/// Inspect synchronization compatibility without modifying the Vault or Git repository.
///
/// # Errors
///
/// Returns [`KbError`] when authoritative Vault configuration cannot be read safely.
pub fn check_sync(
    root: &Path,
    user_paths: &UserPaths,
    overrides: &ConfigOverrides,
) -> Result<SyncReport, KbError> {
    let status = vault_status(root, user_paths, overrides)?;
    let config = load_effective_config(root, user_paths, overrides)?;
    let mut findings = portable_path_findings(root)?;
    findings.extend(conflict_findings(root)?);
    if status.recovery.pending_operations > 0 {
        findings.push(finding(
            "incomplete_operation",
            SyncFindingLevel::Error,
            true,
            Some(".kb/runtime"),
            "This device has an unfinished Knowledge-Brain operation.",
            "Recover or finish the operation before receiving synchronized changes.",
        ));
    }
    for item in lint(root, &config, time::OffsetDateTime::now_utc())?.findings {
        if item.severity == OkfSeverity::Error {
            findings.push(finding(
                &item.code,
                SyncFindingLevel::Error,
                item.code == "duplicate_title"
                    || item.path.as_str() == "Wiki/index.md"
                    || item.path.as_str() == "Wiki/log.md",
                Some(item.path.as_str()),
                item.message,
                item.remediation,
            ));
        }
    }
    if config.search.mode.value == SearchMode::Bm25
        && let Err(error) = crate::bm25::validate_current_index(root, &config)
    {
        findings.push(finding(
            "local_search_index_stale",
            SyncFindingLevel::Warning,
            false,
            Some(".kb/cache/bm25.json"),
            error.message,
            "Run kb cache rebuild on this device, or continue with direct-search fallback.",
        ));
    }
    let (git, git_findings) = inspect_git(root);
    findings.extend(git_findings);
    findings.sort_by(|left, right| {
        (&left.level, &left.code, &left.path).cmp(&(&right.level, &right.code, &right.path))
    });
    let summary = summarize(&findings);
    Ok(SyncReport {
        schema_version: CURRENT_SCHEMA_VERSION,
        vault_id: status.vault_id,
        root: root.to_path_buf(),
        summary,
        git,
        findings,
    })
}

fn portable_path_findings(root: &Path) -> Result<Vec<SyncFinding>, KbError> {
    let mut paths = Vec::new();
    collect_shared_paths(root, root, &mut paths)?;
    let mut findings = Vec::new();
    let mut valid = Vec::new();
    for path in paths {
        let display = path.to_string_lossy().replace('\\', "/");
        match PortableRelativePath::from_path(&path) {
            Ok(_) => valid.push(path),
            Err(error) => findings.push(finding(
                "portable_path_unsafe",
                SyncFindingLevel::Error,
                true,
                Some(&display),
                error.message,
                "Rename the path so it is valid on Windows, macOS, and Linux.",
            )),
        }
    }
    for collision in detect_portability_collisions(&valid)? {
        for path in collision.paths {
            let display = path.to_string_lossy().replace('\\', "/");
            findings.push(finding(
                "portable_path_collision",
                SyncFindingLevel::Error,
                true,
                Some(&display),
                "This shared path collides by portable case or Unicode rules.",
                "Rename one colliding path before synchronizing across operating systems.",
            ));
        }
    }
    Ok(findings)
}

fn collect_shared_paths(
    root: &Path,
    directory: &Path,
    output: &mut Vec<PathBuf>,
) -> Result<(), KbError> {
    let entries = fs::read_dir(directory).map_err(|error| {
        KbError::io_failure(
            "read directory",
            directory.display().to_string(),
            error.to_string(),
        )
    })?;
    for entry in entries {
        let entry = entry.map_err(|error| {
            KbError::io_failure(
                "read directory entry",
                directory.display().to_string(),
                error.to_string(),
            )
        })?;
        let path = entry.path();
        let relative = path.strip_prefix(root).map_err(|error| {
            KbError::invalid_config(path.display().to_string(), error.to_string())
        })?;
        let portable = relative.to_string_lossy().replace('\\', "/");
        if excluded_shared_path(&portable) {
            continue;
        }
        let file_type = entry.file_type().map_err(|error| {
            KbError::io_failure("inspect", path.display().to_string(), error.to_string())
        })?;
        if file_type.is_symlink() {
            continue;
        }
        output.push(relative.to_path_buf());
        if file_type.is_dir() {
            collect_shared_paths(root, &path, output)?;
        }
    }
    Ok(())
}

fn excluded_shared_path(path: &str) -> bool {
    let basename = path.rsplit('/').next().unwrap_or(path);
    path == ".git"
        || path.starts_with(".git/")
        || path == ".kb/config.local.yml"
        || path == ".kb/cache"
        || path.starts_with(".kb/cache/")
        || path == ".kb/runtime"
        || path.starts_with(".kb/runtime/")
        || path == ".obsidian/workspace.json"
        || path == ".obsidian/workspace-mobile.json"
        || path == ".obsidian/workspaces.json"
        || path == ".trash"
        || path.starts_with(".trash/")
        || matches!(basename, ".DS_Store" | "Thumbs.db" | "Desktop.ini")
}

/// Return filesystem conflict findings that make a shared Vault write unsafe.
///
/// # Errors
///
/// Returns [`KbError`] when a managed path cannot be inspected.
pub fn blocking_sync_findings(root: &Path) -> Result<Vec<SyncFinding>, KbError> {
    let mut findings = conflict_findings(root)?;
    findings.extend(
        inspect_git(root)
            .1
            .into_iter()
            .filter(|finding| finding.blocks_writes),
    );
    Ok(findings
        .into_iter()
        .filter(|finding| finding.blocks_writes)
        .collect())
}

/// Reject an actual shared Vault publication when synchronization conflicts are unresolved.
///
/// # Errors
///
/// Returns `sync_conflict` with structured findings when continuing could publish mixed state.
pub fn ensure_shared_write_sync_safe(root: &Path) -> Result<(), KbError> {
    let findings = blocking_sync_findings(root)?;
    if findings.is_empty() {
        return Ok(());
    }
    Err(KbError::new(
        ErrorCode::SyncConflict,
        "Knowledge-Brain cannot write shared Vault files while synchronization conflicts are unresolved.",
        false,
        "Resolve the reported files and run kb sync check again.",
    )
    .with_details(serde_json::json!({ "findings": findings })))
}

fn conflict_findings(root: &Path) -> Result<Vec<SyncFinding>, KbError> {
    let mut paths = vec![
        root.join("KB.md"),
        root.join("admission.yml"),
        root.join(".kb/config.yml"),
    ];
    collect_markdown(&root.join("Wiki"), &mut paths)?;
    let mut findings = Vec::new();
    for path in paths {
        let bytes = fs::read(&path).map_err(|error| {
            KbError::io_failure("read", path.display().to_string(), error.to_string())
        })?;
        let text = String::from_utf8_lossy(&bytes);
        if !text.lines().any(is_conflict_marker) {
            continue;
        }
        let relative = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        let managed = relative == "KB.md"
            || relative == "admission.yml"
            || relative == ".kb/config.yml"
            || relative == "Wiki/index.md"
            || relative == "Wiki/log.md"
            || text.contains("kb:managed:start");
        findings.push(finding(
            "sync_conflict_marker",
            if managed {
                SyncFindingLevel::Error
            } else {
                SyncFindingLevel::Warning
            },
            managed,
            Some(&relative),
            "The file contains unresolved conflict markers.",
            "Resolve the competing versions, then run kb sync check again.",
        ));
    }
    Ok(findings)
}

fn collect_markdown(directory: &Path, paths: &mut Vec<PathBuf>) -> Result<(), KbError> {
    let entries = fs::read_dir(directory).map_err(|error| {
        KbError::io_failure(
            "read directory",
            directory.display().to_string(),
            error.to_string(),
        )
    })?;
    for entry in entries {
        let entry = entry.map_err(|error| {
            KbError::io_failure(
                "read directory entry",
                directory.display().to_string(),
                error.to_string(),
            )
        })?;
        let file_type = entry.file_type().map_err(|error| {
            KbError::io_failure(
                "inspect",
                entry.path().display().to_string(),
                error.to_string(),
            )
        })?;
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            collect_markdown(&entry.path(), paths)?;
        } else if entry
            .path()
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("md"))
        {
            paths.push(entry.path());
        }
    }
    Ok(())
}

fn inspect_git(root: &Path) -> (GitInspection, Vec<SyncFinding>) {
    let output = run_git(root, &["rev-parse", "--show-toplevel"]);
    match output {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => (
            GitInspection {
                state: GitInspectionState::Unavailable,
                repository_root: None,
                message: Some("Git is unavailable; filesystem checks still ran.".into()),
            },
            Vec::new(),
        ),
        Err(error) => (
            GitInspection {
                state: GitInspectionState::Failed,
                repository_root: None,
                message: Some(format!("Git inspection failed: {error}")),
            },
            Vec::new(),
        ),
        Ok(output) if output.status.success() => {
            let repository_root = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim());
            let mut findings = git_repository_findings(root);
            findings
                .sort_by(|left, right| (&left.code, &left.path).cmp(&(&right.code, &right.path)));
            (
                GitInspection {
                    state: GitInspectionState::Checked,
                    repository_root: Some(repository_root),
                    message: None,
                },
                findings,
            )
        }
        Ok(output) => (
            GitInspection {
                state: GitInspectionState::NotRepository,
                repository_root: None,
                message: bounded_message(&output.stderr),
            },
            Vec::new(),
        ),
    }
}

fn git_repository_findings(root: &Path) -> Vec<SyncFinding> {
    let mut findings = Vec::new();
    if let Ok(output) = run_git(root, &["ls-files", "-z", "--unmerged", "--", "."])
        && output.status.success()
    {
        for record in nul_records(&output.stdout) {
            let path = record.rsplit_once('\t').map_or(record, |(_, path)| path);
            findings.push(finding(
                "git_unmerged",
                SyncFindingLevel::Error,
                true,
                Some(path),
                "Git reports an unresolved merge entry.",
                "Resolve the Git conflict, then run kb sync check again.",
            ));
        }
    }

    findings.extend(git_upstream_findings(root));

    let mut arguments = vec!["ls-files", "-z", "--"];
    arguments.extend(LOCAL_PATHS);
    if let Ok(output) = run_git(root, &arguments)
        && output.status.success()
    {
        for path in nul_records(&output.stdout) {
            findings.push(finding(
                "local_state_tracked",
                SyncFindingLevel::Warning,
                path.starts_with(".kb/runtime/"),
                Some(path),
                "A machine-local file is tracked by Git.",
                "Remove it from Git tracking and add the matching portable ignore rule.",
            ));
        }
    }

    let ignore = normalized_rule_lines(&root.join(".gitignore"));
    let missing_ignore = GITIGNORE_LINES
        .iter()
        .filter(|line| !ignore.iter().any(|actual| actual == **line))
        .copied()
        .collect::<Vec<_>>();
    if !missing_ignore.is_empty() {
        findings.push(finding(
            "gitignore_missing_portable_rule",
            SyncFindingLevel::Warning,
            false,
            Some(".gitignore"),
            "The repository does not contain every recommended local-state exclusion.",
            format!("Review and add these lines:\n{}", missing_ignore.join("\n")),
        ));
    }

    let attributes = normalized_rule_lines(&root.join(".gitattributes"));
    let missing_text = GITATTRIBUTES_LINES[..4]
        .iter()
        .filter(|line| !attributes.iter().any(|actual| actual == **line))
        .copied()
        .collect::<Vec<_>>();
    if !missing_text.is_empty() {
        findings.push(finding(
            "gitattributes_text_unstable",
            SyncFindingLevel::Warning,
            false,
            Some(".gitattributes"),
            "Portable text line endings are not fully pinned to LF.",
            format!("Review and add these lines:\n{}", missing_text.join("\n")),
        ));
    }
    if !attributes
        .iter()
        .any(|actual| actual == GITATTRIBUTES_LINES[4])
    {
        findings.push(finding(
            "gitattributes_source_object_rewrite",
            SyncFindingLevel::Warning,
            false,
            Some(".gitattributes"),
            "Immutable source objects are not explicitly protected from Git text rewriting.",
            format!("Review and add this line:\n{}", GITATTRIBUTES_LINES[4]),
        ));
    }
    findings
}

fn git_upstream_findings(root: &Path) -> Vec<SyncFinding> {
    let Ok(upstream) = run_git(
        root,
        &[
            "rev-parse",
            "--abbrev-ref",
            "--symbolic-full-name",
            "@{upstream}",
        ],
    ) else {
        return Vec::new();
    };
    if !upstream.status.success() {
        return Vec::new();
    }
    let upstream = String::from_utf8_lossy(&upstream.stdout).trim().to_owned();
    let Ok(counts) = run_git(
        root,
        &["rev-list", "--left-right", "--count", "HEAD...@{upstream}"],
    ) else {
        return vec![upstream_state_unavailable(&upstream)];
    };
    if !counts.status.success() {
        return vec![upstream_state_unavailable(&upstream)];
    }
    let values = String::from_utf8_lossy(&counts.stdout)
        .split_whitespace()
        .map(str::parse::<u64>)
        .collect::<Result<Vec<_>, _>>();
    let Ok(values) = values else {
        return vec![upstream_state_unavailable(&upstream)];
    };
    let [ahead, behind] = values.as_slice() else {
        return vec![upstream_state_unavailable(&upstream)];
    };
    match (*ahead, *behind) {
        (_, 0) => Vec::new(),
        (0, behind) => vec![finding(
            "git_branch_behind",
            SyncFindingLevel::Error,
            true,
            None,
            format!("The current branch is {behind} commit(s) behind {upstream}."),
            "Update the working tree from its upstream, then run kb sync check again.",
        )],
        (ahead, behind) => vec![finding(
            "git_branch_diverged",
            SyncFindingLevel::Error,
            true,
            None,
            format!(
                "The current branch and {upstream} have diverged ({ahead} local, {behind} upstream commit(s))."
            ),
            "Resolve the Git divergence without force-pushing, then run kb sync check and kb lint again.",
        )],
    }
}

fn upstream_state_unavailable(upstream: &str) -> SyncFinding {
    finding(
        "git_upstream_state_unavailable",
        SyncFindingLevel::Error,
        true,
        None,
        format!("Git could not compare the current branch with {upstream}."),
        "Repair the local Git upstream state, then run kb sync check again.",
    )
}

fn run_git(root: &Path, arguments: &[&str]) -> Result<Output, std::io::Error> {
    let mut command = Command::new("git");
    command
        .arg("--no-optional-locks")
        .arg("-C")
        .arg(root)
        .args(arguments)
        .env_clear();
    for (name, value) in std::env::vars_os() {
        let upper = name.to_string_lossy().to_ascii_uppercase();
        if !["KEY", "SECRET", "TOKEN", "PASSWORD"]
            .iter()
            .any(|needle| upper.contains(needle))
        {
            command.env(name, value);
        }
    }
    command
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("LC_ALL", "C")
        .output()
        .and_then(|output| {
            if output.stdout.len() > MAX_GIT_OUTPUT || output.stderr.len() > MAX_GIT_OUTPUT {
                Err(std::io::Error::other(
                    "Git inspection output exceeded the 1 MiB safety limit",
                ))
            } else {
                Ok(output)
            }
        })
}

fn nul_records(bytes: &[u8]) -> Vec<&str> {
    bytes
        .split(|byte| *byte == 0)
        .filter(|record| !record.is_empty())
        .filter_map(|record| std::str::from_utf8(record).ok())
        .collect()
}

fn normalized_rule_lines(path: &Path) -> Vec<String> {
    fs::read_to_string(path).map_or_else(
        |_| Vec::new(),
        |text| {
            text.lines()
                .map(str::trim)
                .filter(|line| !line.is_empty() && !line.starts_with('#'))
                .map(ToOwned::to_owned)
                .collect()
        },
    )
}

fn bounded_message(bytes: &[u8]) -> Option<String> {
    let message = String::from_utf8_lossy(bytes).trim().to_owned();
    (!message.is_empty()).then_some(message)
}

fn is_conflict_marker(line: &str) -> bool {
    line.starts_with("<<<<<<< ") || line == "=======" || line.starts_with(">>>>>>> ")
}

fn finding(
    code: &str,
    level: SyncFindingLevel,
    blocks_writes: bool,
    path: Option<&str>,
    message: impl Into<String>,
    next_action: impl Into<String>,
) -> SyncFinding {
    SyncFinding {
        code: code.into(),
        level,
        blocks_writes,
        path: path.map(ToOwned::to_owned),
        message: message.into(),
        next_action: next_action.into(),
    }
}

fn summarize(findings: &[SyncFinding]) -> SyncSummary {
    SyncSummary {
        errors: findings
            .iter()
            .filter(|finding| finding.level == SyncFindingLevel::Error)
            .count(),
        warnings: findings
            .iter()
            .filter(|finding| finding.level == SyncFindingLevel::Warning)
            .count(),
        information: findings
            .iter()
            .filter(|finding| finding.level == SyncFindingLevel::Info)
            .count(),
        writes_blocked: findings.iter().any(|finding| finding.blocks_writes),
    }
}
