//! Application workflows over Knowledge-Brain Vaults.

mod adopt;
mod backup;
mod discovery;
mod extract;
mod review;
mod search;
mod skill_assets;
mod skill_hosts;
mod skill_plan;
mod source_apply;
mod source_io;
mod source_plan;
mod source_record;
mod source_verify;
pub use discovery::{DiscoveredSource, DiscoverySnapshot, SkippedSource, discover_sources};
pub use extract::{BuiltinTextExtractor, classify_media_type, extract_bytes};
pub use review::review_sources;
pub use search::{query, rebuild_catalog};
pub use skill_assets::{SkillAsset, skill_assets};
pub use skill_hosts::{
    AgentRoots, DetectedSkillHost, SkillTarget, detect_skill_hosts, resolve_skill_host,
    skill_target,
};
pub use skill_plan::{
    SkillInstallState, SkillPlanRequest, SkillStatusReport, apply_skill_plan, create_skill_plan,
    skill_status,
};
pub use source_plan::{ReviewReport, SourceCapturePlan, SourceCaptureResult};
pub use source_verify::{Verification, VerificationItem, verify_sources};
mod app;
mod bm25;
mod capabilities;
mod config;
mod doctor;
mod init;
mod knowledge_apply;
mod knowledge_plan;
mod lint;
mod lock;
mod managed_markdown;
mod operation;
mod operation_events;
mod operation_summary;
mod registry;
mod schema;
mod status;
mod storage;
mod template;
mod user_dirs;
mod vault;

pub use adopt::{apply_operation, create_adoption_plan};
pub use app::{
    AdmissionRequest, AppContext, AppRequest, AppResponse, BackupRequest, ConfigRequest,
    OperationRequest, SkillRequest, VaultRequest, run,
};
pub use backup::{BackupCreateRequest, create_backup, restore_backup, verify_backup};
pub use capabilities::{Capabilities, capabilities};
pub use config::{
    AdmissionAction, ConfigChange, ConfigOverrides, ConfigTarget, ValidationReport,
    admission_change, config_get, config_set, config_show, config_unset, config_validate,
    edit_admission, edit_config, load_admission, load_effective_config,
};
pub use doctor::{CheckStatus, DoctorCheck, DoctorReport, doctor};
pub use init::{InitReport, InitRequest, init_and_register_vault, init_vault};
pub use knowledge_apply::apply_knowledge;
pub use knowledge_plan::create_knowledge_plan;
pub use lint::{LintReport, lint};
pub use lock::{LockMode, VaultLock};
pub use operation::{AdoptionResult, OperationState, inspect_operation};
pub use operation_events::operation_events;
pub use operation_summary::{
    OperationSummary, attach_operation_summary, summary_for_adoption_plan,
    summary_for_adoption_result, summary_for_knowledge_plan, summary_for_knowledge_result,
    summary_for_skill_plan, summary_for_skill_result, summary_for_source_plan,
    summary_for_source_result, summary_for_state,
};
pub use registry::{
    VaultRecord, VaultRegistry, list_vaults, rebind_vault, register_vault, unregister_vault,
};
pub use status::{
    AdmissionStatus, CacheStatus, RecoveryStatus, SchemaStatus, StatusReport, ValidationState,
    vault_status,
};
pub use storage::atomic_replace;
pub use user_dirs::UserPaths;
pub use vault::{ResolvedVault, VaultSelection, resolve_vault};
