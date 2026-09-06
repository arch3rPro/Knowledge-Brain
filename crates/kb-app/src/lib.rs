//! Application workflows over Knowledge-Brain Vaults.

mod config;
mod init;
mod storage;
mod template;
mod user_dirs;

pub use config::{ConfigOverrides, load_effective_config};
pub use init::{InitReport, InitRequest, init_vault};
pub use storage::atomic_replace;
pub use user_dirs::UserPaths;
