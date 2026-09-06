use std::path::Path;

use crate::{ErrorCode, KbError};

/// Reject symbolic links and Windows reparse points at managed boundaries.
///
/// # Errors
///
/// Returns [`KbError`] when metadata cannot be read or the path is a symbolic
/// link, junction, or another Windows reparse point.
pub fn ensure_not_link_or_reparse_point(path: &Path) -> Result<(), KbError> {
    let metadata = std::fs::symlink_metadata(path).map_err(|error| {
        KbError::new(
            ErrorCode::UnsafePath,
            format!("Cannot inspect {}: {error}", path.display()),
            false,
            "Check that the path exists and is readable.",
        )
    })?;

    if metadata.file_type().is_symlink() || is_windows_reparse_point(&metadata) {
        return Err(KbError::new(
            ErrorCode::UnsafePath,
            format!("Linked paths are not allowed: {}", path.display()),
            false,
            "Use a real directory inside the Vault.",
        ));
    }
    Ok(())
}

#[cfg(windows)]
fn is_windows_reparse_point(metadata: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;

    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
const fn is_windows_reparse_point(_metadata: &std::fs::Metadata) -> bool {
    false
}
