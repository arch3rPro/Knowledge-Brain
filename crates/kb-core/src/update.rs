use std::{fmt, path::PathBuf, str::FromStr};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{KbError, OperationId, SkillHost, SkillScope};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdatePlanState {
    NoChanges,
    ReviewRequired,
    Blocked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdatePhase {
    Preview,
    Confirmed,
    Running,
    Applied,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdateExecutionState {
    Preview,
    Confirmed,
    ReplacingCli,
    CliReplaced,
    ApplyingComponents,
    Completed,
    CompletedWithSkips,
    Partial,
    Failed,
}

impl UpdateExecutionState {
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Completed | Self::CompletedWithSkips | Self::Partial | Self::Failed
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdateComponentKind {
    Executable,
    DataSchema,
    VaultTemplate,
    ManagedSkill,
    SearchIndex,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdateComponentState {
    Pending,
    Applied,
    Unchanged,
    Skipped,
    Stale,
    Failed,
    RebuildRequired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdateScopeMode {
    CurrentVault,
    SelectedVaults,
    RegisteredVaults,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateScope {
    pub mode: UpdateScopeMode,
    pub vaults: Vec<Uuid>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdateFileAction {
    Create,
    Update,
    Delete,
    Replace,
    Rebuild,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdateOwnership {
    Executable,
    WholeFile,
    MarkedRegion,
    ManagedSkill,
    ManagedBridge,
    Symlink,
    RebuildableIndex,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateFileChange {
    pub path: PathBuf,
    pub ownership: UpdateOwnership,
    pub action: UpdateFileAction,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub before_sha256: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after_sha256: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diff: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateConflict {
    pub component_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
    pub reason: String,
    pub next_action: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateComponent {
    pub id: String,
    pub kind: UpdateComponentKind,
    pub state: UpdateComponentState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vault_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host: Option<SkillHost>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skill_scope: Option<SkillScope>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to_version: Option<String>,
    pub changes: Vec<UpdateFileChange>,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdatePlan {
    pub kind: String,
    pub phase: UpdatePhase,
    pub state: UpdatePlanState,
    pub operation_id: OperationId,
    pub current_version: String,
    pub target_version: String,
    pub scope: UpdateScope,
    pub excluded_vaults: Vec<Uuid>,
    pub components: Vec<UpdateComponent>,
    pub conflicts: Vec<UpdateConflict>,
    pub skipped: Vec<String>,
    pub untouched: Vec<String>,
    pub created_at: String,
    pub expires_at: String,
}

impl UpdatePlan {
    /// Validate deterministic ordering and untrusted persisted digest fields.
    ///
    /// # Errors
    ///
    /// Returns an invalid-configuration error for malformed or nondeterministic plans.
    pub fn validate(&self) -> Result<(), KbError> {
        if self.kind != "update"
            || self.current_version.trim().is_empty()
            || self.target_version.trim().is_empty()
            || !strictly_sorted_unique(&self.scope.vaults)
            || !strictly_sorted_unique(&self.excluded_vaults)
            || !strictly_sorted_unique_by(&self.components, |component| component.id.as_str())
            || !self.components.iter().all(valid_component)
        {
            return Err(KbError::invalid_config(
                "update plan",
                "identity, ordering, or digest fields are invalid",
            ));
        }
        Ok(())
    }

    /// Derive the confirmation token from the complete canonical plan.
    ///
    /// # Errors
    ///
    /// Returns an error when the plan is invalid or cannot be serialized.
    pub fn confirmation_token(&self) -> Result<UpdateConfirmationToken, KbError> {
        self.validate()?;
        let bytes = serde_json::to_vec(self)
            .map_err(|error| KbError::invalid_config("update plan", error.to_string()))?;
        Ok(UpdateConfirmationToken(hex::encode(Sha256::digest(bytes))))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateOperation {
    pub kind: String,
    pub phase: UpdatePhase,
    pub operation_id: OperationId,
    pub current_version: String,
    pub target_version: String,
    pub scope: UpdateScope,
    pub components: Vec<UpdateComponent>,
    pub conflicts: Vec<UpdateConflict>,
    pub skipped: Vec<String>,
    pub untouched: Vec<String>,
    pub execution_state: UpdateExecutionState,
    pub created_at: String,
    pub updated_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct UpdateConfirmationToken(String);

impl fmt::Display for UpdateConfirmationToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl FromStr for UpdateConfirmationToken {
    type Err = KbError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if is_sha256(value) {
            Ok(Self(value.to_owned()))
        } else {
            Err(KbError::invalid_config(
                "update confirmation token",
                "expected 64 lowercase hexadecimal characters",
            ))
        }
    }
}

fn valid_component(component: &UpdateComponent) -> bool {
    !component.id.trim().is_empty()
        && !component.message.trim().is_empty()
        && component.changes.iter().all(|change| {
            change.before_sha256.as_deref().is_none_or(is_sha256)
                && change.after_sha256.as_deref().is_none_or(is_sha256)
        })
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn strictly_sorted_unique<T: Ord>(values: &[T]) -> bool {
    values.windows(2).all(|pair| pair[0] < pair[1])
}

fn strictly_sorted_unique_by<T, K: Ord + ?Sized>(values: &[T], key: impl Fn(&T) -> &K) -> bool {
    values.windows(2).all(|pair| key(&pair[0]) < key(&pair[1]))
}
