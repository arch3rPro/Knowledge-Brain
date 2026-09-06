//! Application workflows over Knowledge-Brain Vaults.

mod init;
mod storage;
mod template;

pub use init::{InitReport, InitRequest, init_vault};
pub use storage::atomic_replace;
