use sha2::{Digest, Sha256};
use std::sync::OnceLock;

#[derive(Debug)]
pub struct SkillAsset {
    pub path: &'static str,
    pub bytes: &'static [u8],
    pub sha256: String,
}

static ASSETS: OnceLock<Vec<SkillAsset>> = OnceLock::new();

#[must_use]
pub fn skill_assets() -> &'static [SkillAsset] {
    ASSETS.get_or_init(|| {
        [
            (
                "SKILL.md",
                include_bytes!("../../../skills/knowledge-brain/SKILL.md").as_slice(),
            ),
            (
                "references/maintenance.md",
                include_bytes!("../../../skills/knowledge-brain/references/maintenance.md").as_slice(),
            ),
            (
                "references/query.md",
                include_bytes!("../../../skills/knowledge-brain/references/query.md").as_slice(),
            ),
            (
                "references/review-and-save.md",
                include_bytes!(
                    "../../../skills/knowledge-brain/references/review-and-save.md"
                )
                .as_slice(),
            ),
        ]
        .into_iter()
        .map(|(path, bytes)| SkillAsset {
            path,
            bytes,
            sha256: hex::encode(Sha256::digest(bytes)),
        })
        .collect()
    })
}

