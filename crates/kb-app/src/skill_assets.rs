use sha2::{Digest, Sha256};
use std::sync::OnceLock;

#[derive(Debug)]
pub struct SkillAsset {
    pub path: &'static str,
    pub bytes: &'static [u8],
    pub sha256: String,
}

static ASSETS: OnceLock<Vec<SkillAsset>> = OnceLock::new();
static LEGACY_ASSETS: OnceLock<Vec<SkillAsset>> = OnceLock::new();

pub const SKILL_NAMES: [&str; 8] = [
    "kb-vault",
    "kb-config",
    "kb-ingest",
    "kb-query",
    "kb-save",
    "kb-ops",
    "kb-backup",
    "kb-connect",
];

#[must_use]
pub fn skill_assets() -> &'static [SkillAsset] {
    ASSETS.get_or_init(|| {
        make_assets([
            (
                "kb-vault/SKILL.md",
                include_bytes!("../../../skills/kb-vault/SKILL.md").as_slice(),
            ),
            (
                "kb-config/SKILL.md",
                include_bytes!("../../../skills/kb-config/SKILL.md").as_slice(),
            ),
            (
                "kb-ingest/SKILL.md",
                include_bytes!("../../../skills/kb-ingest/SKILL.md").as_slice(),
            ),
            (
                "kb-query/SKILL.md",
                include_bytes!("../../../skills/kb-query/SKILL.md").as_slice(),
            ),
            (
                "kb-save/SKILL.md",
                include_bytes!("../../../skills/kb-save/SKILL.md").as_slice(),
            ),
            (
                "kb-ops/SKILL.md",
                include_bytes!("../../../skills/kb-ops/SKILL.md").as_slice(),
            ),
            (
                "kb-backup/SKILL.md",
                include_bytes!("../../../skills/kb-backup/SKILL.md").as_slice(),
            ),
            (
                "kb-connect/SKILL.md",
                include_bytes!("../../../skills/kb-connect/SKILL.md").as_slice(),
            ),
        ])
    })
}

#[must_use]
pub fn legacy_skill_assets() -> &'static [SkillAsset] {
    LEGACY_ASSETS.get_or_init(|| {
        make_assets([
            (
                "SKILL.md",
                include_bytes!("../assets/legacy-skill/SKILL.md").as_slice(),
            ),
            (
                "references/maintenance.md",
                include_bytes!("../assets/legacy-skill/references/maintenance.md").as_slice(),
            ),
            (
                "references/query.md",
                include_bytes!("../assets/legacy-skill/references/query.md").as_slice(),
            ),
            (
                "references/review-and-save.md",
                include_bytes!("../assets/legacy-skill/references/review-and-save.md").as_slice(),
            ),
        ])
    })
}

fn make_assets<const N: usize>(assets: [(&'static str, &'static [u8]); N]) -> Vec<SkillAsset> {
    assets
        .into_iter()
        .map(|(path, bytes)| SkillAsset {
            path,
            bytes,
            sha256: hex::encode(Sha256::digest(bytes)),
        })
        .collect()
}
