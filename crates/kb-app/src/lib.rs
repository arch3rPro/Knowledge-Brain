//! Application workflows over Knowledge-Brain Vaults.

mod adopt;
mod discovery;
mod extract;
mod review;
mod search;
mod source_apply;
mod source_io;
mod source_plan;
mod source_record;
mod source_verify;
pub use discovery::{DiscoveredSource, DiscoverySnapshot, SkippedSource, discover_sources};
pub use extract::{BuiltinTextExtractor, classify_media_type, extract_bytes};
pub use review::review_sources;
pub use search::{query, rebuild_catalog};
pub use source_plan::{ReviewReport, SourceCapturePlan, SourceCaptureResult};
pub use source_verify::{Verification, VerificationItem, verify_sources};
mod app;
mod capabilities;
mod config;
mod doctor;
mod init;
mod knowledge_plan;
mod lint;
mod lock;
mod managed_markdown;
mod operation;
mod registry;
mod status;
mod storage;
mod template;
mod user_dirs;
mod vault;

pub use adopt::{apply_operation, create_adoption_plan};
pub use app::{
    AdmissionRequest, AppContext, AppRequest, AppResponse, ConfigRequest, OperationRequest,
    VaultRequest, run,
};
pub use capabilities::{Capabilities, capabilities};
pub use config::{
    AdmissionAction, ConfigChange, ConfigOverrides, ConfigTarget, ValidationReport,
    admission_change, config_get, config_set, config_show, config_unset, config_validate,
    edit_admission, edit_config, load_admission, load_effective_config,
};
pub use doctor::{CheckStatus, DoctorCheck, DoctorReport, doctor};
pub use init::{InitReport, InitRequest, init_and_register_vault, init_vault};
pub use knowledge_plan::create_knowledge_plan;
pub use lint::{LintReport, lint};
pub use lock::{LockMode, VaultLock};
pub use operation::{AdoptionResult, OperationState, inspect_operation};
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
