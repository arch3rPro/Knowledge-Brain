use directories::BaseDirs;
use kb_core::{ErrorCode, KbError, SkillHost, SkillScope};
use serde::Serialize;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRoots {
    pub home_dir: PathBuf,
    pub config_dir: PathBuf,
}

impl AgentRoots {
    #[must_use]
    pub const fn new(home_dir: PathBuf, config_dir: PathBuf) -> Self {
        Self {
            home_dir,
            config_dir,
        }
    }

    /// Resolve user and configuration roots from the operating system.
    ///
    /// # Errors
    ///
    /// Returns an error when system roots are unavailable or an override is
    /// empty or relative.
    pub fn resolve(environment: &BTreeMap<String, String>) -> Result<Self, KbError> {
        let base = BaseDirs::new().ok_or_else(|| {
            KbError::invalid_config(
                "agent user directories",
                "operating-system home and configuration directories are unavailable",
            )
        })?;
        Ok(Self::new(
            override_path(environment, "KB_AGENT_HOME")?
                .unwrap_or_else(|| base.home_dir().to_path_buf()),
            override_path(environment, "KB_AGENT_CONFIG_DIR")?
                .unwrap_or_else(|| base.config_dir().to_path_buf()),
        ))
    }
}

fn override_path(
    environment: &BTreeMap<String, String>,
    key: &str,
) -> Result<Option<PathBuf>, KbError> {
    let Some(value) = environment.get(key) else {
        return Ok(None);
    };
    let path = PathBuf::from(value);
    if value.trim().is_empty() || !path.is_absolute() {
        return Err(KbError::invalid_config(
            key,
            "path must be a nonempty absolute path",
        ));
    }
    Ok(Some(path))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SkillTarget {
    pub host: SkillHost,
    pub scope: SkillScope,
    pub skills_root: PathBuf,
    pub legacy_skill_dir: PathBuf,
    pub bridge_file: Option<PathBuf>,
}

/// Resolve the Skill and bridge locations for one host and scope.
///
/// # Errors
///
/// Returns an error unless every supplied root is absolute.
pub fn skill_target(
    vault_root: &Path,
    roots: &AgentRoots,
    host: SkillHost,
    scope: SkillScope,
) -> Result<SkillTarget, KbError> {
    if !vault_root.is_absolute() || !roots.home_dir.is_absolute() || !roots.config_dir.is_absolute()
    {
        return Err(KbError::new(
            ErrorCode::UnsafePath,
            "Skill targets require absolute Vault and user roots.",
            false,
            "Resolve the Vault and operating-system user directories before installing a Skill.",
        ));
    }

    if host == SkillHost::Hermes && scope == SkillScope::Vault {
        return Err(KbError::new(
            ErrorCode::CapabilityUnavailable,
            "Hermes project Skills require a trusted Git project, but a Knowledge-Brain Vault does not require Git.",
            false,
            "Install Hermes Skills with --scope user, or configure and trust a project directly in Hermes.",
        ));
    }

    let (skills_root, bridge_file) = match (scope, host) {
        (SkillScope::Vault, SkillHost::Codex) => (
            vault_root.join(".agents/skills"),
            Some(vault_root.join("AGENTS.md")),
        ),
        (SkillScope::Vault, SkillHost::ClaudeCode) => (
            vault_root.join(".claude/skills"),
            Some(vault_root.join("CLAUDE.md")),
        ),
        (SkillScope::Vault, SkillHost::GeminiCli) => (
            vault_root.join(".gemini/skills"),
            Some(vault_root.join("GEMINI.md")),
        ),
        (SkillScope::Vault, SkillHost::OpenCode) => (
            vault_root.join(".opencode/skills"),
            Some(vault_root.join("AGENTS.md")),
        ),
        (SkillScope::User, SkillHost::Codex) => {
            let base = roots.home_dir.join(".codex");
            (base.join("skills"), Some(base.join("AGENTS.md")))
        }
        (SkillScope::User, SkillHost::ClaudeCode) => {
            let base = roots.home_dir.join(".claude");
            (base.join("skills"), Some(base.join("CLAUDE.md")))
        }
        (SkillScope::User, SkillHost::GeminiCli) => {
            let base = roots.home_dir.join(".gemini");
            (base.join("skills"), Some(base.join("GEMINI.md")))
        }
        (SkillScope::User, SkillHost::OpenCode) => {
            let base = roots.config_dir.join("opencode");
            (base.join("skills"), Some(base.join("AGENTS.md")))
        }
        (SkillScope::Vault, SkillHost::OpenClaw) => (vault_root.join("skills"), None),
        (SkillScope::User, SkillHost::OpenClaw) => (roots.home_dir.join(".openclaw/skills"), None),
        (SkillScope::User, SkillHost::Hermes) => (roots.home_dir.join(".hermes/skills"), None),
        (SkillScope::Vault, SkillHost::DeepSeekHarness) => (vault_root.join(".dsh/skills"), None),
        (SkillScope::User, SkillHost::DeepSeekHarness) => {
            (roots.home_dir.join(".dsh/skills"), None)
        }
        (SkillScope::Vault, SkillHost::Pi) => (vault_root.join(".pi/skills"), None),
        (SkillScope::User, SkillHost::Pi) => (roots.home_dir.join(".pi/agent/skills"), None),
        (SkillScope::Vault, SkillHost::Hermes) => unreachable!("handled above"),
    };
    Ok(SkillTarget {
        host,
        scope,
        legacy_skill_dir: skills_root.join("knowledge-brain"),
        skills_root,
        bridge_file,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DetectedSkillHost {
    pub host: SkillHost,
    pub evidence: Vec<String>,
}

/// Detect supported Agent hosts from unambiguous files in a Vault root.
///
/// # Errors
///
/// Returns an error when the root is not an existing directory.
pub fn detect_skill_hosts(root: &Path) -> Result<Vec<DetectedSkillHost>, KbError> {
    if !root.is_dir() {
        return Err(KbError::new(
            ErrorCode::VaultNotFound,
            format!(
                "Skill detection root is not a directory: {}",
                root.display()
            ),
            false,
            "Select an initialized Vault directory.",
        ));
    }
    let mut found = BTreeMap::<SkillHost, Vec<String>>::new();
    let mut mark = |host, path: &str| {
        if root.join(path).exists() {
            found.entry(host).or_default().push(path.to_owned());
        }
    };
    mark(SkillHost::ClaudeCode, ".claude");
    mark(SkillHost::ClaudeCode, "CLAUDE.md");
    mark(SkillHost::GeminiCli, ".gemini");
    mark(SkillHost::GeminiCli, "GEMINI.md");
    mark(SkillHost::OpenCode, ".opencode");
    mark(SkillHost::OpenCode, "opencode.json");
    mark(SkillHost::Hermes, ".hermes");
    mark(SkillHost::DeepSeekHarness, ".dsh");
    mark(SkillHost::Pi, ".pi");
    if root.join("AGENTS.md").exists() {
        found
            .entry(SkillHost::Codex)
            .or_default()
            .push("AGENTS.md".into());
        found
            .entry(SkillHost::OpenCode)
            .or_default()
            .push("AGENTS.md".into());
    }
    Ok(found
        .into_iter()
        .map(|(host, evidence)| DetectedSkillHost { host, evidence })
        .collect())
}

/// Select an explicit host or the only detected host.
///
/// # Errors
///
/// Returns an error when automatic detection is empty or ambiguous.
pub fn resolve_skill_host(
    explicit: Option<SkillHost>,
    detected: &[DetectedSkillHost],
) -> Result<SkillHost, KbError> {
    if let Some(host) = explicit {
        return Ok(host);
    }
    match detected {
        [only] => Ok(only.host),
        [] => Err(KbError::invalid_config(
            "auto Skill host",
            "no supported Agent host was detected; pass --host explicitly",
        )),
        _ => Err(KbError::invalid_config(
            "auto Skill host",
            "host detection is ambiguous; pass --host explicitly",
        )),
    }
}
