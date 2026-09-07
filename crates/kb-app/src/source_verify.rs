use crate::{
    source_io::{Budget, hash, safe_path},
    source_record,
};
use kb_core::{EffectiveConfig, ErrorCode, KbError};
use serde::Serialize;
use std::{collections::BTreeSet, path::Path};
#[derive(Serialize)]
pub struct Verification {
    pub checks: Vec<VerificationItem>,
}
#[derive(Serialize)]
pub struct VerificationItem {
    pub path: String,
    pub status: crate::CheckStatus,
    pub message: String,
}
/// Check every historical captured object without changing it.
/// # Errors
/// Fails on an unreadable or malformed managed source record.
pub fn verify_sources(root: &Path, c: &EffectiveConfig) -> Result<Verification, KbError> {
    let mut b = Budget::new(c);
    let mut checks = Vec::new();
    let mut seen = BTreeSet::new();
    for (_, stored) in source_record::inventory(root, c, &mut b)? {
        let mut versions = stored.record.versions;
        versions.push(stored.record.source);
        for v in versions {
            let path = v.object_path();
            if !seen.insert(path.clone()) {
                continue;
            }
            let result = safe_path(root, &path)
                .and_then(|p| b.read(&p))
                .and_then(|bytes| {
                    if hash(&bytes) == v.sha256() {
                        Ok(())
                    } else {
                        Err(crate::source_apply::integrity(&path))
                    }
                });
            let (status, message) = match result {
                Ok(()) => (
                    crate::CheckStatus::Pass,
                    "Original object matches SHA-256.".into(),
                ),
                Err(e) if e.code == ErrorCode::LimitExceeded => {
                    (crate::CheckStatus::NotChecked, e.message)
                }
                Err(e) => (crate::CheckStatus::Fail, e.message),
            };
            checks.push(VerificationItem {
                path,
                status,
                message,
            });
        }
    }
    Ok(Verification { checks })
}
