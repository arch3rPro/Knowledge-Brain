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
    ] {
        assert!(help.contains(&format!("  {command}")), "{command}");
        assert!(
            COMMAND_REFERENCE.contains(&format!("kb {command}")),
            "{command}"
        );
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
