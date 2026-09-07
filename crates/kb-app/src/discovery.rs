use crate::{
    classify_media_type,
    source_io::{Budget, hash, io, safe_path},
};
use globset::{GlobBuilder, GlobSet, GlobSetBuilder};
use kb_core::{
    AdmissionDocument, AdmissionEntry, EffectiveConfig, KbError, MediaType, PortableRelativePath,
    SourceId, SourceVersion, portability_key,
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs, path::Path};
#[derive(Debug, Clone)]
pub struct DiscoveredSource {
    pub version: SourceVersion,
    pub size: u64,
    pub media_type: MediaType,
    pub bytes: Vec<u8>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkippedSource {
    pub path: String,
    pub reason: String,
}
pub struct DiscoverySnapshot {
    pub sources: Vec<DiscoveredSource>,
    pub skipped: Vec<SkippedSource>,
}
/// Read the permitted local sources and hash their exact bytes.
/// # Errors
/// Rejects invalid filters, unsafe paths, IO failures and configured size/count limits.
pub fn discover_sources(
    root: &Path,
    admission: &AdmissionDocument,
    config: &EffectiveConfig,
) -> Result<DiscoverySnapshot, KbError> {
    let mut snapshot = DiscoverySnapshot {
        sources: Vec::new(),
        skipped: Vec::new(),
    };
    let mut budget = Budget::new(config);
    for entry in admission.directories.iter().filter(|e| e.enabled) {
        if entry.path.eq_ignore_ascii_case(".git") || entry.path.eq_ignore_ascii_case(".kb") {
            snapshot.skipped.push(SkippedSource {
                path: entry.path.clone(),
                reason: "framework".into(),
            });
            continue;
        }
        let include = compile(&entry.effective_include())?;
        let exclude = compile(&entry.effective_exclude())?;
        let dir = safe_path(root, &entry.path)?;
        let mut walk = Walker {
            root: &dir,
            entry,
            config,
            include,
            exclude,
            budget: &mut budget,
            snapshot: &mut snapshot,
            seen: BTreeMap::new(),
        };
        walk.visit(&dir)?;
    }
    snapshot
        .sources
        .sort_by_key(|s| s.version.source.logical_uri());
    Ok(snapshot)
}
fn compile(patterns: &[&str]) -> Result<GlobSet, KbError> {
    let mut b = GlobSetBuilder::new();
    for p in patterns {
        b.add(
            GlobBuilder::new(p)
                .literal_separator(true)
                .backslash_escape(false)
                .build()
                .map_err(|e| KbError::invalid_config("admission glob", e.to_string()))?,
        );
    }
    b.build()
        .map_err(|e| KbError::invalid_config("admission glob", e.to_string()))
}
pub(crate) fn includes_path(
    entry: &AdmissionEntry,
    config: &EffectiveConfig,
    path: &str,
) -> Result<bool, KbError> {
    let include = compile(&entry.effective_include())?;
    let exclude = compile(&entry.effective_exclude())?;
    let mut prefix = String::new();
    for part in path.split('/') {
        if part.eq_ignore_ascii_case(".git")
            || part.eq_ignore_ascii_case(".kb")
            || (part.starts_with('.') && !config.files.include_hidden.value)
        {
            return Ok(false);
        }
        if !prefix.is_empty() {
            prefix.push('/');
        }
        prefix.push_str(part);
        if exclude.is_match(&prefix) || exclude.is_match(format!("{prefix}/")) {
            return Ok(false);
        }
    }
    Ok(include.is_match(path))
}
struct Walker<'a> {
    root: &'a Path,
    entry: &'a AdmissionEntry,
    config: &'a EffectiveConfig,
    include: GlobSet,
    exclude: GlobSet,
    budget: &'a mut Budget,
    snapshot: &'a mut DiscoverySnapshot,
    seen: BTreeMap<String, String>,
}
impl Walker<'_> {
    fn skip(&mut self, p: &str, reason: &str) {
        self.snapshot.skipped.push(SkippedSource {
            path: format!("{}/{}", self.entry.path, p),
            reason: reason.into(),
        });
    }
    fn visit(&mut self, dir: &Path) -> Result<(), KbError> {
        let mut entries = Vec::new();
        for entry in fs::read_dir(dir).map_err(|e| io("read admitted directory", dir, e))? {
            let entry = entry.map_err(|e| io("read entry", dir, e))?;
            self.budget.visit(&entry.path())?;
            entries.push(entry);
        }
        entries.sort_by_key(fs::DirEntry::file_name);
        for e in entries {
            let p = e.path();
            let rel = p
                .strip_prefix(self.root)
                .map_err(|e| io("resolve", &p, e))?;
            let name = e.file_name();
            let name = name
                .to_str()
                .ok_or_else(|| KbError::invalid_config("source path", "not UTF-8"))?;
            let text = PortableRelativePath::from_path(rel)?.as_str().to_owned();
            // Disappearing or unreadable entries are IO errors, not ignored links.
            fs::symlink_metadata(&p).map_err(|e| io("inspect source", &p, e))?;
            if kb_core::ensure_not_link_or_reparse_point(&p).is_err() {
                self.skip(&text, "link");
                continue;
            }
            if name.eq_ignore_ascii_case(".git") || name.eq_ignore_ascii_case(".kb") {
                self.skip(&text, "framework");
                continue;
            }
            if name.starts_with('.') && !self.config.files.include_hidden.value {
                self.skip(&text, "hidden");
                continue;
            }
            let path = PortableRelativePath::parse(&text)?;
            if let Some(old) = self.seen.insert(portability_key(&path), text.clone()) {
                return Err(KbError::invalid_config(
                    "source paths",
                    format!("{old} collides with {text}"),
                ));
            }
            if self.exclude.is_match(&text) || self.exclude.is_match(format!("{text}/")) {
                self.skip(&text, "excluded");
                continue;
            }
            let kind = e.file_type().map_err(|err| io("inspect", &p, err))?;
            if kind.is_dir() {
                self.visit(&p)?;
                continue;
            }
            if !kind.is_file() {
                self.skip(&text, "not_regular");
                continue;
            }
            if !self.include.is_match(&text) {
                self.skip(&text, "not_included");
                continue;
            }
            let bytes = self.budget.read(&p)?;
            self.snapshot.sources.push(DiscoveredSource {
                version: SourceVersion::new(SourceId::new(&self.entry.id, path)?, hash(&bytes))?,
                size: bytes.len() as u64,
                media_type: classify_media_type(&p),
                bytes,
            });
        }
        Ok(())
    }
}
