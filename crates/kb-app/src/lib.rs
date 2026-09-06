//! Application workflows over Knowledge-Brain Vaults.

mod config;
mod init;
mod storage;
mod template;
mod user_dirs;

pub use config::{
    AdmissionAction, ConfigChange, ConfigOverrides, ConfigTarget, ValidationReport,
    admission_change, config_get, config_set, config_show, config_unset, config_validate,
    edit_admission, edit_config, load_admission, load_effective_config,
};
pub use init::{InitReport, InitRequest, init_vault};
pub use storage::atomic_replace;
pub use user_dirs::UserPaths;
