use kb_core::{EffectiveConfig, ErrorCode, KbError, PortableRelativePath, portability_key};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::Read,
    path::{Path, PathBuf},
};
pub(crate) fn hash(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
pub(crate) fn io(action: &str, p: &Path, e: impl std::fmt::Display) -> KbError {
    KbError::io_failure(action, p.display().to_string(), e.to_string())
}
pub(crate) fn safe_path(root: &Path, relative: &str) -> Result<PathBuf, KbError> {
    let p = PortableRelativePath::parse(relative)?;
    kb_core::ensure_not_link_or_reparse_point(root)?;
    let mut path = root.to_path_buf();
    for part in p.as_str().split('/') {
        path.push(part);
        match fs::symlink_metadata(&path) {
            Ok(_) => kb_core::ensure_not_link_or_reparse_point(&path)?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(io("inspect", &path, e)),
        }
    }
    Ok(path)
}
pub(crate) fn limit(name: &str, max: u64, path: &Path) -> KbError {
    KbError::new(
        ErrorCode::LimitExceeded,
        format!("{name} exceeds {max} at {}", path.display()),
        false,
        "Adjust the configured limit or narrow the admitted files.",
    )
    .with_details(serde_json::json!({"limit":name,"maximum":max,"path":path}))
}
pub(crate) struct Budget {
    file: u64,
    total: u64,
    count: u64,
    read: u64,
    visited: u64,
}
impl Budget {
    pub fn new(c: &EffectiveConfig) -> Self {
        Self {
            file: c.limits.max_file_bytes.value,
            total: c.limits.max_total_read_bytes.value,
            count: c.limits.max_files_per_review.value,
            read: 0,
            visited: 0,
        }
    }
    pub fn visit(&mut self, p: &Path) -> Result<(), KbError> {
        self.visited += 1;
        if self.visited > self.count {
            return Err(limit("limits.max_files_per_review", self.count, p));
        }
        Ok(())
    }
    pub fn read(&mut self, p: &Path) -> Result<Vec<u8>, KbError> {
        kb_core::ensure_not_link_or_reparse_point(p)?;
        if !fs::symlink_metadata(p)
            .map_err(|e| io("inspect", p, e))?
            .is_file()
        {
            return Err(KbError::new(
                ErrorCode::UnsafePath,
                "Expected a regular file before opening.",
                false,
                "Replace special files with regular files.",
            ));
        }
        let f = fs::File::open(p).map_err(|e| io("open", p, e))?;
        let m = f.metadata().map_err(|e| io("inspect", p, e))?;
        if !m.is_file() {
            return Err(KbError::new(
                ErrorCode::UnsafePath,
                "Expected a regular source file.",
                false,
                "Use a regular file.",
            ));
        }
        let allowed = self.file.min(self.total.saturating_sub(self.read));
        let mut bytes = Vec::new();
        f.take(allowed.saturating_add(1))
            .read_to_end(&mut bytes)
            .map_err(|e| io("read", p, e))?;
        if bytes.len() as u64 > self.file {
            return Err(limit("limits.max_file_bytes", self.file, p));
        }
        self.read = self.read.saturating_add(bytes.len() as u64);
        if self.read > self.total {
            return Err(limit("limits.max_total_read_bytes", self.total, p));
        }
        Ok(bytes)
    }
}
pub(crate) fn list_files(
    root: &Path,
    relative: &str,
    budget: &mut Budget,
) -> Result<Vec<PortableRelativePath>, KbError> {
    let start = safe_path(root, relative)?;
    if !start.exists() {
        return Ok(Vec::new());
    }
    let mut files = Vec::new();
    let mut seen = BTreeMap::new();
    walk(root, &start, budget, &mut files, &mut seen)?;
    files.sort();
    Ok(files)
}
fn walk(
    root: &Path,
    dir: &Path,
    b: &mut Budget,
    out: &mut Vec<PortableRelativePath>,
    seen: &mut BTreeMap<String, String>,
) -> Result<(), KbError> {
    let mut entries = Vec::new();
    for entry in fs::read_dir(dir).map_err(|e| io("read directory", dir, e))? {
        let entry = entry.map_err(|e| io("read entry", dir, e))?;
        b.visit(&entry.path())?;
        entries.push(entry);
    }
    entries.sort_by_key(fs::DirEntry::file_name);
    for entry in entries {
        let p = entry.path();
        kb_core::ensure_not_link_or_reparse_point(&p)?;
        let rel = PortableRelativePath::from_path(
            p.strip_prefix(root).map_err(|e| io("resolve", &p, e))?,
        )?;
        if let Some(old) = seen.insert(portability_key(&rel), rel.as_str().into()) {
            return Err(KbError::invalid_config(
                "paths",
                format!("{old} collides with {}", rel.as_str()),
            ));
        }
        if entry
            .file_type()
            .map_err(|e| io("inspect", &p, e))?
            .is_dir()
        {
            walk(root, &p, b, out, seen)?;
        } else {
            out.push(rel);
        }
    }
    Ok(())
}
pub(crate) fn read_config_hash(c: &EffectiveConfig) -> Result<String, KbError> {
    serde_json::to_vec(&serde_json::json!({"limits":[c.limits.max_file_bytes.value,c.limits.max_files_per_review.value,c.limits.max_total_read_bytes.value],"hidden":c.files.include_hidden.value}))
        .map(|v|hash(&v)).map_err(|e|KbError::invalid_config("read config",e.to_string()))
}
