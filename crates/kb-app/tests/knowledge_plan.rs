use std::{collections::BTreeMap, fs, path::Path};

use kb_app::{
    ConfigOverrides, InitRequest, OperationState, UserPaths, apply_knowledge,
    create_knowledge_plan, init_vault, inspect_operation, lint, load_effective_config,
};
use kb_core::{
    CURRENT_SCHEMA_VERSION, ErrorCode, KnowledgeChangeRequest, KnowledgePlanRequest,
    PortableRelativePath,
};
use sha2::{Digest, Sha256};
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
            kind: kb_core::KnowledgeChangeKind::Upsert,
            from_path: None,
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

#[test]
fn plan_rejects_duplicate_titles_in_the_resulting_index_partition() {
    let (_temporary, vault, paths, config) = setup();
    fs::write(
        vault.join("Wiki/research/existing.md"),
        managed_article("Shared title", None).replace("type: Article", "type: Research"),
    )
    .unwrap();
    let mut duplicate = request("research/new.md", None);
    duplicate.changes[0].content =
        managed_article("  shared   TITLE  ", None).replace("type: Article", "type: Research");

    let error = create_knowledge_plan(&vault, &paths, &config, duplicate, now()).unwrap_err();

    assert_eq!(error.code, ErrorCode::InvalidConfig);
    assert_eq!(
        error.details.as_ref().unwrap()["finding_code"],
        "duplicate_title"
    );
    assert_eq!(error.details.as_ref().unwrap()["partition"], "research");
    assert_eq!(
        error.details.as_ref().unwrap()["paths"],
        serde_json::json!(["research/existing.md", "research/new.md"])
    );
    assert!(!paths.state_dir.join("operations").exists());
}

#[test]
fn plan_allows_the_same_title_in_different_index_partitions() {
    let (_temporary, vault, paths, config) = setup();
    fs::write(
        vault.join("Wiki/research/shared.md"),
        managed_article("Shared title", None).replace("type: Article", "type: Research"),
    )
    .unwrap();
    let mut same_title = request("articles/shared.md", None);
    same_title.changes[0].content = managed_article("shared title", None);

    let plan = create_knowledge_plan(&vault, &paths, &config, same_title, now()).unwrap();

    assert!(
        plan.writes
            .iter()
            .any(|write| write.path.as_str() == "Wiki/articles/shared.md")
    );
}

#[test]
fn plan_rejects_duplicate_titles_created_by_the_same_request() {
    let (_temporary, vault, paths, config) = setup();
    let mut duplicate = request("articles/first.md", None);
    duplicate.changes.push(KnowledgeChangeRequest {
        kind: kb_core::KnowledgeChangeKind::Upsert,
        from_path: None,
        path: PortableRelativePath::parse("articles/second.md").unwrap(),
        before_sha256: None,
        summary: "Add another explanation.".into(),
        content: managed_article("LOCAL-FIRST", None),
    });

    let error = create_knowledge_plan(&vault, &paths, &config, duplicate, now()).unwrap_err();

    assert_eq!(error.code, ErrorCode::InvalidConfig);
    assert_eq!(
        error.details.as_ref().unwrap()["finding_code"],
        "duplicate_title"
    );
    assert_eq!(
        error.details.as_ref().unwrap()["paths"],
        serde_json::json!(["articles/first.md", "articles/second.md"])
    );
    assert!(!paths.state_dir.join("operations").exists());
}

#[test]
fn external_research_requires_an_admitted_exact_source_version() {
    let (_temporary, vault, paths, config) = setup();
    let mut external = request("research/external.md", None);
    external.changes[0].content = external.changes[0]
        .content
        .replace("type: Article", "type: Research")
        .replace(
            "  managed: true",
            "  managed: true\n  origin: external_research",
        );

    let error = create_knowledge_plan(&vault, &paths, &config, external, now()).unwrap_err();

    assert_eq!(error.code, ErrorCode::InvalidConfig);
    assert_eq!(
        error.details.as_ref().unwrap()["finding_code"],
        "source_admission_required"
    );
    assert!(!paths.state_dir.join("operations").exists());
}

#[test]
fn declared_original_knowledge_does_not_require_a_fabricated_source() {
    let (_temporary, vault, paths, config) = setup();
    let mut original = request("articles/original.md", None);
    original.changes[0].content = original.changes[0]
        .content
        .replace(
            "sources:\n  - id: source\n    resource: https://example.com/source\n",
            "",
        )
        .replace("  managed: true", "  managed: true\n  origin: original");

    let plan = create_knowledge_plan(&vault, &paths, &config, original, now()).unwrap();

    assert!(
        plan.writes
            .iter()
            .any(|write| write.path.as_str() == "Wiki/articles/original.md")
    );
}

#[test]
fn external_research_accepts_an_existing_exact_source_version() {
    let (_temporary, vault, paths, config) = setup();
    let digest = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let exact_uri = write_source_record(&vault, digest);
    let mut external = request("research/external.md", None);
    external.changes[0].content = external.changes[0]
        .content
        .replace("type: Article", "type: Research")
        .replace("https://example.com/source", &exact_uri)
        .replace(
            "  managed: true",
            "  managed: true\n  origin: external_research",
        );

    let plan = create_knowledge_plan(&vault, &paths, &config, external, now()).unwrap();

    assert_eq!(plan.source_versions, vec![exact_uri]);
}

#[test]
fn one_knowledge_operation_deletes_a_page_and_updates_managed_navigation() {
    let (_temporary, vault, paths, config) = setup();
    let target = vault.join("Wiki/articles/obsolete.md");
    let content = managed_article("Obsolete", None);
    fs::write(&target, &content).unwrap();
    let request: KnowledgePlanRequest = serde_json::from_value(serde_json::json!({
        "schema_version": "v1.0",
        "changes": [{
            "kind": "delete",
            "path": "articles/obsolete.md",
            "before_sha256": hex::encode(Sha256::digest(content.as_bytes())),
            "summary": "Remove obsolete knowledge."
        }]
    }))
    .unwrap();

    let plan =
        create_knowledge_plan(&vault, &paths, &config, request, OffsetDateTime::now_utc()).unwrap();
    let result = apply_knowledge(&paths, plan.operation_id, &ConfigOverrides::default()).unwrap();

    assert!(!target.exists());
    assert!(
        result
            .changed
            .iter()
            .any(|path| path.as_str() == "Wiki/articles/obsolete.md")
    );
    assert!(
        !fs::read_to_string(vault.join("Wiki/index.md"))
            .unwrap()
            .contains("obsolete.md")
    );
    assert!(
        fs::read_to_string(vault.join("Wiki/log.md"))
            .unwrap()
            .contains("**Deletion**")
    );
    let report = lint(&vault, &config, OffsetDateTime::now_utc()).unwrap();
    assert!(!report.findings.iter().any(|finding| {
        finding.code == "broken_link" && finding.path.as_str() == "Wiki/log.md"
    }));
}

#[test]
fn one_knowledge_operation_moves_a_page_without_leaving_the_old_path() {
    let (_temporary, vault, paths, config) = setup();
    let source = vault.join("Wiki/research/old-name.md");
    let old_content = managed_article("Old name", None).replace("type: Article", "type: Research");
    fs::write(&source, &old_content).unwrap();
    let moved_content =
        managed_article("Better name", None).replace("type: Article", "type: Research");
    let request: KnowledgePlanRequest = serde_json::from_value(serde_json::json!({
        "schema_version": "v1.0",
        "changes": [{
            "kind": "move",
            "from_path": "research/old-name.md",
            "path": "research/better-name.md",
            "before_sha256": hex::encode(Sha256::digest(old_content.as_bytes())),
            "summary": "Rename the research page.",
            "content": moved_content
        }]
    }))
    .unwrap();

    let plan =
        create_knowledge_plan(&vault, &paths, &config, request, OffsetDateTime::now_utc()).unwrap();
    apply_knowledge(&paths, plan.operation_id, &ConfigOverrides::default()).unwrap();

    assert!(!source.exists());
    assert!(vault.join("Wiki/research/better-name.md").is_file());
    let index = fs::read_to_string(vault.join("Wiki/index.md")).unwrap();
    assert!(!index.contains("old-name.md"));
    assert!(index.contains("[Better name](research/better-name.md)"));
    let log = fs::read_to_string(vault.join("Wiki/log.md")).unwrap();
    assert!(log.contains("**Move**"));
    assert!(log.contains("research/old-name.md"));
}

#[test]
fn delete_rejects_a_wiki_page_that_is_not_managed() {
    let (_temporary, vault, paths, config) = setup();
    let content = "---\ntype: Article\ntitle: Human page\n---\n\n# Human page\n";
    let target = vault.join("Wiki/articles/human.md");
    fs::write(&target, content).unwrap();
    let request: KnowledgePlanRequest = serde_json::from_value(serde_json::json!({
        "schema_version": "v1.0",
        "changes": [{
            "kind": "delete",
            "path": "articles/human.md",
            "before_sha256": hex::encode(Sha256::digest(content.as_bytes())),
            "summary": "Delete a human-owned page."
        }]
    }))
    .unwrap();

    let error = create_knowledge_plan(&vault, &paths, &config, request, now()).unwrap_err();

    assert_eq!(error.code, ErrorCode::InvalidConfig);
    assert!(target.is_file());
    assert!(!paths.state_dir.join("operations").exists());
}

#[test]
fn deleting_one_legacy_duplicate_title_is_allowed_by_the_final_state_check() {
    let (_temporary, vault, paths, config) = setup();
    let keep = managed_article("Duplicate", None);
    let remove = managed_article("Duplicate", None);
    fs::write(vault.join("Wiki/articles/keep.md"), &keep).unwrap();
    fs::write(vault.join("Wiki/articles/remove.md"), &remove).unwrap();
    let request: KnowledgePlanRequest = serde_json::from_value(serde_json::json!({
        "schema_version": "v1.0",
        "changes": [{
            "kind": "delete",
            "path": "articles/remove.md",
            "before_sha256": hex::encode(Sha256::digest(remove.as_bytes())),
            "summary": "Remove one legacy duplicate."
        }]
    }))
    .unwrap();

    let plan =
        create_knowledge_plan(&vault, &paths, &config, request, OffsetDateTime::now_utc()).unwrap();
    apply_knowledge(&paths, plan.operation_id, &ConfigOverrides::default()).unwrap();

    assert!(vault.join("Wiki/articles/keep.md").is_file());
    assert!(!vault.join("Wiki/articles/remove.md").exists());
}

#[test]
fn moving_a_page_rejects_a_duplicate_title_in_the_destination_partition() {
    let (_temporary, vault, paths, config) = setup();
    let existing = managed_article("Collision", None).replace("type: Article", "type: Research");
    let source = managed_article("Collision", None).replace("type: Article", "type: Research");
    fs::write(vault.join("Wiki/research/existing.md"), existing).unwrap();
    fs::write(vault.join("Wiki/research/source.md"), &source).unwrap();
    let request: KnowledgePlanRequest = serde_json::from_value(serde_json::json!({
        "schema_version": "v1.0",
        "changes": [{
            "kind": "move",
            "from_path": "research/source.md",
            "path": "research/moved.md",
            "before_sha256": hex::encode(Sha256::digest(source.as_bytes())),
            "summary": "Move a colliding title.",
            "content": source
        }]
    }))
    .unwrap();

    let error = create_knowledge_plan(&vault, &paths, &config, request, now()).unwrap_err();

    assert_eq!(error.code, ErrorCode::InvalidConfig);
    assert_eq!(error.details.unwrap()["finding_code"], "duplicate_title");
    assert!(vault.join("Wiki/research/source.md").is_file());
    assert!(!paths.state_dir.join("operations").exists());
}

fn write_source_record(vault: &Path, digest: &str) -> String {
    let logical = "kb-source://notes/a.md";
    let record_digest = hex::encode(Sha256::digest(logical.as_bytes()));
    let path = vault.join(format!(
        "Wiki/external-sources/records/{}/{}.md",
        &record_digest[..2],
        record_digest
    ));
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(
        path,
        format!(
            "---\ntype: Reference\ntitle: a.md\nkb:\n  source:\n    source:\n      source:\n        admission_id: notes\n        relative_path: a.md\n      sha256: {digest}\n    title: a.md\n    size: 1\n    media_type: markdown\n    extraction_status: text_ready\n    present: true\n    captured_at: 2026-09-07T03:00:00Z\n    versions:\n      - source:\n          admission_id: notes\n          relative_path: a.md\n        sha256: {digest}\n---\n\n# a.md\n"
        ),
    )
    .unwrap();
    format!("{logical}?sha256={digest}")
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
