use std::{collections::BTreeMap, fs, path::Path};

use kb_app::{
    ConfigOverrides, InitRequest, OperationState, UserPaths, create_knowledge_plan, init_vault,
    inspect_operation, load_effective_config,
};
use kb_core::{
    CURRENT_SCHEMA_VERSION, ErrorCode, KnowledgeChangeRequest, KnowledgePlanRequest,
    PortableRelativePath,
};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

fn setup() -> (
    tempfile::TempDir,
    std::path::PathBuf,
    UserPaths,
    kb_core::EffectiveConfig,
) {
    let temporary = tempfile::tempdir().unwrap();
    let vault = temporary.path().join("vault");
    init_vault(&InitRequest {
        target: vault.clone(),
    })
    .unwrap();
    let paths = UserPaths::new(
        temporary.path().join("config"),
        temporary.path().join("state"),
        temporary.path().join("cache"),
    );
    let config = load_effective_config(&vault, &paths, &ConfigOverrides::default()).unwrap();
    (temporary, vault, paths, config)
}

fn now() -> OffsetDateTime {
    OffsetDateTime::parse("2026-09-07T12:00:00+08:00", &Rfc3339).unwrap()
}

fn managed_article(title: &str, description: Option<&str>) -> String {
    let description =
        description.map_or_else(String::new, |value| format!("description: {value}\n"));
    format!(
        "---\ntype: Article\ntitle: {title}\n{description}status: stable\ngenerated:\n  by: process:test\n  at: 2026-09-07T03:00:00Z\nsources:\n  - id: source\n    resource: https://example.com/source\nkb:\n  managed: true\n---\n\n# {title}\n"
    )
}

fn request(path: &str, before_sha256: Option<String>) -> KnowledgePlanRequest {
    KnowledgePlanRequest {
        schema_version: CURRENT_SCHEMA_VERSION,
        changes: vec![KnowledgeChangeRequest {
            path: PortableRelativePath::parse(path).unwrap(),
            before_sha256,
            summary: "Add the durable explanation.".into(),
            content: managed_article("Local-first", Some("Files remain authoritative.")),
        }],
    }
}

#[test]
fn plan_derives_index_log_diff_and_persists_without_changing_vault() {
    let (_temporary, vault, paths, config) = setup();
    fs::write(
        vault.join("Wiki/index.md"),
        "# Human index\n\nKeep this.\n\n<!-- kb:managed:start -->\n<!-- kb:managed:end -->\n",
    )
    .unwrap();
    fs::write(
        vault.join("Wiki/log.md"),
        "# Human log\n\nKeep this too.\n\n<!-- kb:managed:start -->\n<!-- kb:managed:end -->\n",
    )
    .unwrap();
    fs::write(
        vault.join("Wiki/research/existing.md"),
        managed_article("Earlier research", None).replace("type: Article", "type: Research"),
    )
    .unwrap();
    let before = snapshot(&vault);

    let plan = create_knowledge_plan(
        &vault,
        &paths,
        &config,
        request("articles/local-first.md", None),
        now(),
    )
    .unwrap();

    assert_eq!(snapshot(&vault), before);
    assert_eq!(plan.writes.len(), 3);
    let article = write(&plan, "Wiki/articles/local-first.md");
    assert!(article.content.contains("kb:\n  managed: true"));
    let index = write(&plan, "Wiki/index.md");
    assert!(index.content.contains("# Human index\n\nKeep this."));
    assert!(index.content.contains("## Research"));
    assert!(
        index
            .content
            .contains("[Earlier research](research/existing.md)")
    );
    assert!(index.content.contains("## Articles"));
    assert!(
        index
            .content
            .contains("[Local-first](articles/local-first.md) — Files remain authoritative.")
    );
    let log = write(&plan, "Wiki/log.md");
    assert!(log.content.contains("# Human log\n\nKeep this too."));
    assert!(log.content.contains("## 2026-09-07"));
    assert!(log.content.contains("**Creation**"));
    assert!(log.content.contains("Add the durable explanation."));
    assert!(plan.diff.contains("--- /dev/null"));
    assert!(plan.diff.contains("+++ Wiki/articles/local-first.md"));

    let directory = paths
        .state_dir
        .join("operations")
        .join(plan.operation_id.to_string());
    assert!(directory.join("plan.json").is_file());
    assert!(directory.join("plan.sha256").is_file());
    assert!(matches!(
        inspect_operation(&paths, plan.operation_id).unwrap(),
        OperationState::PlannedKnowledge(stored) if stored == plan
    ));
}

#[test]
fn plan_requires_agent_observed_hash_and_strict_managed_content() {
    let (_temporary, vault, paths, config) = setup();
    let target = vault.join("Wiki/articles/current.md");
    fs::write(&target, managed_article("Current", None)).unwrap();
    let original = fs::read(&target).unwrap();

    let error = create_knowledge_plan(
        &vault,
        &paths,
        &config,
        request("articles/current.md", None),
        now(),
    )
    .unwrap_err();
    assert_eq!(error.code, ErrorCode::PlanStale);

    let mut invalid = request("articles/new.md", None);
    invalid.changes[0].content = "---\ntype: Article\n---\n".into();
    let error = create_knowledge_plan(&vault, &paths, &config, invalid, now()).unwrap_err();
    assert_eq!(error.code, ErrorCode::InvalidConfig);
    assert_eq!(fs::read(&target).unwrap(), original);
}

#[test]
fn plan_rejects_corrupt_managed_markers_and_missing_exact_sources() {
    let (_temporary, vault, paths, config) = setup();
    fs::write(
        vault.join("Wiki/index.md"),
        "# Index\n<!-- kb:managed:start -->\n",
    )
    .unwrap();
    let error = create_knowledge_plan(
        &vault,
        &paths,
        &config,
        request("articles/new.md", None),
        now(),
    )
    .unwrap_err();
    assert_eq!(error.code, ErrorCode::InvalidConfig);

    fs::write(
        vault.join("Wiki/index.md"),
        "# Index\n<!-- kb:managed:start -->\n<!-- kb:managed:end -->\n",
    )
    .unwrap();
    let mut missing_source = request("articles/new.md", None);
    missing_source.changes[0].content = missing_source.changes[0].content.replace(
        "https://example.com/source",
        "kb-source://notes/missing.md?sha256=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    );
    let error = create_knowledge_plan(&vault, &paths, &config, missing_source, now()).unwrap_err();
    assert_eq!(error.code, ErrorCode::InvalidConfig);
}

fn write<'a>(plan: &'a kb_core::KnowledgePlan, path: &str) -> &'a kb_core::KnowledgeWrite {
    plan.writes
        .iter()
        .find(|write| write.path.as_str() == path)
        .unwrap()
}

fn snapshot(root: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut output = BTreeMap::new();
    visit(root, root, &mut output);
    output
}

fn visit(root: &Path, directory: &Path, output: &mut BTreeMap<String, Vec<u8>>) {
    let mut entries = fs::read_dir(directory)
        .unwrap()
        .map(Result::unwrap)
        .collect::<Vec<_>>();
    entries.sort_by_key(fs::DirEntry::file_name);
    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            visit(root, &path, output);
        } else {
            output.insert(
                path.strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/"),
                fs::read(path).unwrap(),
            );
        }
    }
}
