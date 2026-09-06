//! Application workflows over Knowledge-Brain Vaults.

mod config;
mod init;
mod registry;
mod storage;
mod template;
mod user_dirs;
mod vault;

pub use config::{
    AdmissionAction, ConfigChange, ConfigOverrides, ConfigTarget, ValidationReport,
    admission_change, config_get, config_set, config_show, config_unset, config_validate,
    edit_admission, edit_config, load_admission, load_effective_config,
};
pub use init::{InitReport, InitRequest, init_and_register_vault, init_vault};
pub use registry::{
    VaultRecord, VaultRegistry, list_vaults, rebind_vault, register_vault, unregister_vault,
};
pub use storage::atomic_replace;
pub use user_dirs::UserPaths;
pub use vault::{ResolvedVault, VaultSelection, resolve_vault};
