use assert_cmd::Command;

const COMMAND_REFERENCE: &str = include_str!("../../../docs/reference/commands.md");
const PROJECT_README: &str = include_str!("../../../README.md");
const ROADMAP: &str = include_str!("../../../ROADMAP.md");

#[test]
fn project_readme_covers_the_first_run_contract() {
    for section in [
        "## Features",
        "## Vault 如何组织",
        "## 快速开始",
        "## 架构",
        "## 开发",
        "## 文档",
    ] {
        assert!(PROJECT_README.contains(section), "missing {section}");
    }

    assert!(!PROJECT_README.contains("## 当前能力"));

    for contract in [
        "admission.yml",
        "Wiki/external-sources",
        "Wiki/research",
        "Wiki/articles",
        "docs/reference/commands.md",
        "docs/reference/configuration.md",
        "ROADMAP.md",
    ] {
        assert!(PROJECT_README.contains(contract), "missing {contract}");
    }

    for stage in ["Stage 1", "Stage 2", "Stage 3", "Stage 4", "Stage 5"] {
        assert!(ROADMAP.contains(stage), "missing {stage}");
    }
}

#[test]
fn command_reference_names_every_real_top_level_command() {
    let output = Command::cargo_bin("kb")
        .unwrap()
        .arg("--help")
        .output()
        .unwrap();
    assert!(output.status.success());
    let help = String::from_utf8(output.stdout).unwrap();

    for command in [
        "backup",
        "init",
        "adopt",
        "apply",
        "plan",
        "operation",
        "config",
        "status",
        "doctor",
        "vault",
        "paths",
        "version",
        "capabilities",
        "skills",
        "mcp",
    ] {
        assert!(help.contains(&format!("  {command}")), "{command}");
        assert!(
            COMMAND_REFERENCE.contains(&format!("kb {command}")),
            "{command}"
        );
    }
}

#[test]
fn agent_skill_and_mcp_references_own_their_public_contracts() {
    let skill = include_str!("../../../docs/reference/agent-skill.md");
    for contract in [
        "kb skills detect",
        "kb skills install",
        "kb skills status",
        "kb skills uninstall",
        "Codex",
        "Claude Code",
        "Gemini CLI",
        "OpenCode",
        "operation",
    ] {
        assert!(
            skill.contains(contract),
            "missing Skill contract: {contract}"
        );
    }

    let mcp = include_str!("../../../docs/reference/mcp.md");
    for contract in [
        "kb mcp",
        "--allow-write",
        "kb_query",
        "kb_plan_knowledge",
        "kb_apply_operation",
        "固定 Vault",
        "stdin",
        "stdout",
    ] {
        assert!(mcp.contains(contract), "missing MCP contract: {contract}");
    }
}

#[test]
fn knowledge_plan_reference_owns_the_save_contract() {
    let reference = include_str!("../../../docs/reference/knowledge-plans.md");
    for contract in [
        "KnowledgePlanRequest",
        "kb plan create",
        "before_sha256",
        "kb.managed: true",
        "knowledge-pending.json",
        "vault_needs_recovery",
        "MCP、HTTP、WebUI 和 GUI",
    ] {
        assert!(reference.contains(contract), "missing {contract}");
    }
}

#[test]
fn search_reference_documents_the_optional_backend_contract() {
    let reference = include_str!("../../../docs/reference/search.md");
    for contract in [
        "bm25f-v1",
        "--strict-backend",
        "index_stale",
        "score_micros",
        "整次请求回退",
    ] {
        assert!(reference.contains(contract), "missing {contract}");
    }
    assert!(COMMAND_REFERENCE.contains("--strict-backend"));
}

#[test]
fn backup_reference_owns_archive_and_restore_boundaries() {
    let reference = include_str!("../../../docs/reference/backup.md");
    for contract in [
        "kb backup create",
        "--without-source-objects",
        "complete_source_evidence",
        "manifest.json",
        "空目录",
        "不可信输入",
    ] {
        assert!(reference.contains(contract), "missing {contract}");
    }
}

#[test]
fn reference_names_every_config_and_admission_subcommand() {
    for command in ["show", "get", "set", "unset", "validate"] {
        assert!(
            COMMAND_REFERENCE.contains(&format!("kb config {command}")),
            "config {command}"
        );
    }
    for command in ["list", "add", "enable", "disable", "remove"] {
        assert!(
            COMMAND_REFERENCE.contains(&format!("kb config admission {command}")),
            "config admission {command}"
        );
    }
}
