use kb_app::skill_assets;
use std::collections::BTreeSet;

#[test]
fn embedded_skill_is_portable_and_complete() {
    let assets = skill_assets();
    let paths = assets
        .iter()
        .map(|asset| asset.path)
        .collect::<BTreeSet<_>>();
    assert_eq!(
        paths,
        BTreeSet::from([
            "SKILL.md",
            "references/maintenance.md",
            "references/query.md",
            "references/review-and-save.md",
        ])
    );

    let entry = assets.iter().find(|asset| asset.path == "SKILL.md").unwrap();
    let text = std::str::from_utf8(entry.bytes).unwrap();
    assert!(text.starts_with("---\nname: knowledge-brain\n"));
    assert!(text.contains("references/query.md"));
    assert!(text.contains("references/review-and-save.md"));
    assert!(text.contains("references/maintenance.md"));

    for asset in assets {
        assert!(!asset.path.starts_with('/'));
        assert_eq!(asset.sha256.len(), 64);
        let text = std::str::from_utf8(asset.bytes).unwrap();
        assert!(!text.contains("/Users/"));
        assert!(!text.contains("C:\\Users\\"));
        assert!(!text.contains("Codex"));
        assert!(!text.contains("Claude"));
    }
}
