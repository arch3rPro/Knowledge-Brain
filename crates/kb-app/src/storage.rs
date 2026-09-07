use std::{io::Write, path::Path};

use atomic_write_file::AtomicWriteFile;
use kb_core::KbError;

pub(crate) fn create_new(path: &Path, bytes: &[u8]) -> Result<(), KbError> {
    let parent = path
        .parent()
        .ok_or_else(|| KbError::invalid_config("destination", "missing parent"))?;
    let mut file = tempfile::NamedTempFile::new_in(parent)
        .map_err(|e| crate::source_io::io("create staging file", path, e))?;
    file.write_all(bytes)
        .map_err(|e| crate::source_io::io("write staging file", path, e))?;
    file.as_file()
        .sync_all()
        .map_err(|e| crate::source_io::io("sync staging file", path, e))?;
    file.persist_noclobber(path)
        .map_err(|e| crate::source_io::io("create without replacing", path, e.error))?;
    Ok(())
}

/// Replace a file only after all new bytes have been written and synchronized.
///
/// # Errors
///
/// Returns [`KbError`] when the destination cannot be opened, written,
/// synchronized, or replaced. The previous destination remains available when
/// the underlying atomic replacement fails before commit.
pub fn atomic_replace(path: &Path, bytes: &[u8]) -> Result<(), KbError> {
    let path_text = path.display().to_string();
    let mut file = AtomicWriteFile::open(path).map_err(|error| {
        KbError::io_failure("open temporary file for", &path_text, error.to_string())
    })?;
    file.write_all(bytes)
        .map_err(|error| KbError::io_failure("write", &path_text, error.to_string()))?;
    file.sync_all()
        .map_err(|error| KbError::io_failure("synchronize", &path_text, error.to_string()))?;
    file.commit()
        .map_err(|error| KbError::io_failure("replace", path_text, error.to_string()))
}
