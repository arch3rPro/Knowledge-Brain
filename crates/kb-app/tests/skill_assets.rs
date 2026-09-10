use kb_app::{SKILL_NAMES, legacy_skill_assets, skill_assets};
use std::{collections::BTreeSet, fs, path::Path};

#[test]
fn embedded_skills_are_the_eight_canonical_task_scoped_sources() {
    assert_eq!(
        SKILL_NAMES,
        [
            "kb-vault",
            "kb-config",
            "kb-ingest",
            "kb-query",
            "kb-save",
            "kb-ops",
            "kb-backup",
            "kb-connect",
        ]
    );
    let assets = skill_assets();
    let paths = assets.iter().map(|asset| asset.path).collect::<Vec<_>>();
    assert_eq!(
        paths,
        vec![
            "kb-vault/SKILL.md",
            "kb-config/SKILL.md",
            "kb-ingest/SKILL.md",
            "kb-query/SKILL.md",
            "kb-save/SKILL.md",
            "kb-save/references/request-format.md",
            "kb-ops/SKILL.md",
            "kb-backup/SKILL.md",
            "kb-connect/SKILL.md",
        ]
    );
    assert_eq!(assets.len(), 9);

    let expected_actions = [
        ("kb-vault", "kb vault list|register|rebind|unregister"),
        ("kb-config", "kb config show --sources"),
        ("kb-ingest", "kb source save --vault"),
        ("kb-query", "This Skill is read-only"),
        ("kb-save", "kb knowledge save <request.json>"),
        ("kb-ops", "kb maintain --vault"),
        ("kb-backup", "kb backup verify"),
        ("kb-connect", "kb skills detect"),
    ];
    for (name, required_action) in expected_actions {
        let asset = assets
            .iter()
            .find(|asset| asset.path == format!("{name}/SKILL.md"))
            .unwrap();
        assert!(!asset.path.starts_with('/'));
        assert_eq!(asset.path, format!("{name}/SKILL.md"));
        assert_eq!(asset.sha256.len(), 64);
        let text = std::str::from_utf8(asset.bytes).unwrap();
        assert!(text.starts_with(&format!("---\nname: {name}\n")));
        assert!(text.contains("\nlicense: MIT\n"));
        assert!(text.contains("\ncompatibility: Requires the portable Knowledge-Brain kb CLI"));
        assert!(text.contains("\n## "));
        assert!(
            text.contains(required_action),
            "{name}: missing {required_action}"
        );
        assert!(!text.contains("/Users/"));
        assert!(!text.contains("C:\\Users\\"));
        assert!(!text.contains("Codex"));
        assert!(!text.contains("Claude"));
    }
    let reference = assets
        .iter()
        .find(|asset| asset.path == "kb-save/references/request-format.md")
        .unwrap();
    assert!(
        std::str::from_utf8(reference.bytes)
            .unwrap()
            .contains("before_sha256")
    );
}

#[test]
fn embedded_assets_are_byte_identical_to_their_named_canonical_sources() {
    let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    for asset in skill_assets() {
        assert_eq!(
            asset.bytes,
            fs::read(repository.join("skills").join(asset.path)).unwrap(),
            "{} must embed its own canonical source",
            asset.path
        );
    }
}

#[test]
fn published_skill_tree_contains_exactly_the_eight_discoverable_skills() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../skills");
    let published = fs::read_dir(root)
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| entry.path().join("SKILL.md"))
        .filter(|path| path.is_file())
        .map(|path| {
            path.strip_prefix(Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))
                .unwrap()
                .to_path_buf()
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(
        published,
        BTreeSet::from_iter(
            SKILL_NAMES.map(|name| Path::new("skills").join(name).join("SKILL.md"))
        )
    );
}

#[test]
fn trigger_evaluation_corpus_covers_every_skill_and_boundary() {
    let corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/agent-skill-trigger-cases.json"
    ))
    .unwrap();
    let cases = corpus.as_object().unwrap();
    assert_eq!(
        cases.keys().cloned().collect::<BTreeSet<_>>(),
        SKILL_NAMES.map(str::to_owned).into_iter().collect()
    );
    for name in SKILL_NAMES {
        let case = &cases[name];
        assert!(
            case["positive"]
                .as_array()
                .is_some_and(|items| items.len() >= 2)
        );
        assert!(
            case["adjacent"]
                .as_array()
                .is_some_and(|items| !items.is_empty())
        );
        assert!(
            case["negative"]
                .as_array()
                .is_some_and(|items| !items.is_empty())
        );
    }

    let ordinary_research =
        serde_json::Value::String("调研 Matt Pocock Skills 并在合适主题目录整理笔记".to_owned());
    for name in ["kb-ingest", "kb-save"] {
        assert!(
            cases[name]["negative"]
                .as_array()
                .unwrap()
                .contains(&ordinary_research),
            "{name} must not trigger for ordinary research notes"
        );
    }
}

#[test]
fn legacy_assets_remain_available_only_as_the_frozen_migration_fixture() {
    let assets = legacy_skill_assets();
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
    let hashes = assets
        .iter()
        .map(|asset| (asset.path, asset.sha256.as_str()))
        .collect::<BTreeSet<_>>();
    assert_eq!(
        hashes,
        BTreeSet::from([
            (
                "SKILL.md",
                "af7bbeeca794d2ba04db134920f44e5e2dc2deb5a9e1a776c934fdabba0ddbae",
            ),
            (
                "references/maintenance.md",
                "76cd5b57cf53b4b4cd2d7a2c66d49ade367b510e29deedf73a76e808de0ca669",
            ),
            (
                "references/query.md",
                "0a07a02c19ba37973a9d081cb1c7804b5a6e93982ae1ca9d3d04b751927fdfc0",
            ),
            (
                "references/review-and-save.md",
                "3defa31a30fd1a2cfc18faeff1507d1d067d183c7c0d9fa03b826e109a2940b3",
            ),
        ])
    );
    let entry = assets
        .iter()
        .find(|asset| asset.path == "SKILL.md")
        .unwrap();
    assert!(
        std::str::from_utf8(entry.bytes)
            .unwrap()
            .starts_with("---\nname: knowledge-brain\n")
    );
}
