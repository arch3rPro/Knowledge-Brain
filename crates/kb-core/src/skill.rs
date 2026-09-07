use crate::{OperationId, OperationKind, SchemaVersion};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SkillHost {
    Codex,
    ClaudeCode,
    GeminiCli,
    OpenCode,
}

impl SkillHost {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::ClaudeCode => "claude-code",
            Self::GeminiCli => "gemini-cli",
            Self::OpenCode => "opencode",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillScope {
    Vault,
    User,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillInstallMode {
    Copy,
    Symlink,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillAction {
    Install,
    Uninstall,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillFileChange {
    pub path: PathBuf,
    pub before_sha256: Option<String>,
    pub after: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillLinkChange {
    pub path: PathBuf,
    pub target: PathBuf,
    pub create: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillPlan {
    pub schema_version: SchemaVersion,
    pub operation_id: OperationId,
    pub kind: OperationKind,
    pub vault_id: Uuid,
    pub vault_root: PathBuf,
    pub host: SkillHost,
    pub scope: SkillScope,
    pub mode: SkillInstallMode,
    pub action: SkillAction,
    pub files: Vec<SkillFileChange>,
    pub link: Option<SkillLinkChange>,
    pub created_at: String,
    pub app_version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillApplyResult {
    pub kind: OperationKind,
    pub operation_id: OperationId,
    pub vault_id: Uuid,
    pub vault_root: PathBuf,
    pub host: SkillHost,
    pub scope: SkillScope,
    pub mode: SkillInstallMode,
    pub action: SkillAction,
    pub changed: Vec<PathBuf>,
    pub warnings: Vec<String>,
}
