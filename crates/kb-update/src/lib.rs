//! Verification primitives for official Knowledge-Brain binary releases.

mod http;
mod identity;
mod install;
mod release;
mod replace;
mod verify;

pub use http::{ReleaseTransport, UreqTransport};
pub use identity::{BuildIdentity, ReleaseTarget, UpdateIdentityError};
pub use install::validate_staged_identity;
pub use release::{
    AvailableRelease, RELEASES_LATEST_URL, UpdateCheck, UpdateError, check_for_update,
    parse_latest_release,
};
pub use replace::{
    ReplaceRequest, parent_process_start_time, remove_update_stage, replace_with_backup,
    wait_for_parent_exit,
};
pub use verify::{VerifiedArchive, verify_release};
