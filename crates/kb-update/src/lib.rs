//! Verification primitives for official Knowledge-Brain binary releases.

mod identity;
mod http;
mod release;
mod verify;

pub use identity::{BuildIdentity, ReleaseTarget, UpdateIdentityError};
pub use http::{ReleaseTransport, UreqTransport};
pub use release::{
    AvailableRelease, RELEASES_LATEST_URL, UpdateCheck, UpdateError, check_for_update,
    parse_latest_release,
};
pub use verify::{VerifiedArchive, verify_release};
