use kb_app::{AppContext, AppRequest, InitRequest, SkillRequest, init_vault};
use kb_core::{ErrorCode, OperationId, SkillHost, SkillInstallMode, SkillScope};
use serde_json::Value;
use std::{collections::BTreeMap, fs, path::Path};

#[test]
fn copy_install_and_uninstall_preserve_user_bridge_bytes() {
    let temp = tempfile::tempdir().unwrap();
    let vault = temp.path().join("vault");
    init_vault(&InitRequest {
        target: vault.clone(),
    })
    .unwrap();
    let original = "# Existing rules\n\nKeep this byte-for-byte.\n";
    fs::write(vault.join("AGENTS.md"), original).unwrap();
    let context = context(temp.path());

    let plan = kb_app::run(
        AppRequest::Skills(SkillRequest::Install {
            vault: Some(vault.to_string_lossy().into_owned()),
            host: Some(SkillHost::Codex),
            scope: SkillScope::Vault,
            mode: SkillInstallMode::Copy,
        }),
        &context,
    )
    .unwrap();
    assert!(!vault.join(".agents/skills/knowledge-brain").exists());
    let install_id = operation_id(&plan);

    kb_app::run(
        AppRequest::Apply {
            operation_id: install_id,
        },
        &context,
    )
    .unwrap();
    assert!(
        vault
            .join(".agents/skills/knowledge-brain/SKILL.md")
            .is_file()
    );
    let bridge = fs::read_to_string(vault.join("AGENTS.md")).unwrap();
    assert!(bridge.starts_with(original));
    assert_eq!(bridge.matches("<!-- knowledge-brain:start -->").count(), 1);

    let status = kb_app::run(
        AppRequest::Skills(SkillRequest::Status {
            vault: Some(vault.to_string_lossy().into_owned()),
            host: Some(SkillHost::Codex),
            scope: SkillScope::Vault,
        }),
        &context,
    )
    .unwrap();
    assert_eq!(status["state"], "current");

    let uninstall = kb_app::run(
        AppRequest::Skills(SkillRequest::Uninstall {
            vault: Some(vault.to_string_lossy().into_owned()),
            host: Some(SkillHost::Codex),
            scope: SkillScope::Vault,
        }),
        &context,
    )
    .unwrap();
    kb_app::run(
        AppRequest::Apply {
            operation_id: operation_id(&uninstall),
        },
        &context,
    )
    .unwrap();
    assert!(!vault.join(".agents/skills/knowledge-brain").exists());
    assert_eq!(
        fs::read_to_string(vault.join("AGENTS.md")).unwrap(),
        original
    );
}

#[test]
fn uninstall_refuses_a_user_modified_asset() {
    let temp = tempfile::tempdir().unwrap();
    let vault = temp.path().join("vault");
    init_vault(&InitRequest {
        target: vault.clone(),
    })
    .unwrap();
    let context = context(temp.path());
    let plan = kb_app::run(
        AppRequest::Skills(SkillRequest::Install {
            vault: Some(vault.to_string_lossy().into_owned()),
            host: Some(SkillHost::GeminiCli),
            scope: SkillScope::Vault,
            mode: SkillInstallMode::Copy,
        }),
        &context,
    )
    .unwrap();
    kb_app::run(
        AppRequest::Apply {
            operation_id: operation_id(&plan),
        },
        &context,
    )
    .unwrap();
    fs::write(
        vault.join(".gemini/skills/knowledge-brain/SKILL.md"),
        "user modification\n",
    )
    .unwrap();

    let status = kb_app::run(
        AppRequest::Skills(SkillRequest::Status {
            vault: Some(vault.to_string_lossy().into_owned()),
            host: Some(SkillHost::GeminiCli),
            scope: SkillScope::Vault,
        }),
        &context,
    )
    .unwrap();
    assert_eq!(status["state"], "modified");
    let error = kb_app::run(
        AppRequest::Skills(SkillRequest::Uninstall {
            vault: Some(vault.to_string_lossy().into_owned()),
            host: Some(SkillHost::GeminiCli),
            scope: SkillScope::Vault,
        }),
        &context,
    )
    .unwrap_err();
    assert_eq!(error.code, ErrorCode::PlanStale);
}

#[test]
fn apply_continues_from_a_verified_partial_install() {
    let temp = tempfile::tempdir().unwrap();
    let vault = temp.path().join("vault");
    init_vault(&InitRequest {
        target: vault.clone(),
    })
    .unwrap();
    let context = context(temp.path());
    let plan = kb_app::run(
        AppRequest::Skills(SkillRequest::Install {
            vault: Some(vault.to_string_lossy().into_owned()),
            host: Some(SkillHost::OpenCode),
            scope: SkillScope::Vault,
            mode: SkillInstallMode::Copy,
        }),
        &context,
    )
    .unwrap();
    let first = &plan["files"][0];
    let path = first["path"].as_str().unwrap();
    fs::create_dir_all(Path::new(path).parent().unwrap()).unwrap();
    fs::write(path, first["after"].as_str().unwrap()).unwrap();

    kb_app::run(
        AppRequest::Apply {
            operation_id: operation_id(&plan),
        },
        &context,
    )
    .unwrap();
    let status = kb_app::run(
        AppRequest::Skills(SkillRequest::Status {
            vault: Some(vault.to_string_lossy().into_owned()),
            host: Some(SkillHost::OpenCode),
            scope: SkillScope::Vault,
        }),
        &context,
    )
    .unwrap();
    assert_eq!(status["state"], "current");
}

#[cfg(unix)]
#[test]
fn explicit_symlink_mode_materializes_a_canonical_copy() {
    let temp = tempfile::tempdir().unwrap();
    let vault = temp.path().join("vault");
    init_vault(&InitRequest {
        target: vault.clone(),
    })
    .unwrap();
    let context = context(temp.path());
    let plan = kb_app::run(
        AppRequest::Skills(SkillRequest::Install {
            vault: Some(vault.to_string_lossy().into_owned()),
            host: Some(SkillHost::ClaudeCode),
            scope: SkillScope::Vault,
            mode: SkillInstallMode::Symlink,
        }),
        &context,
    )
    .unwrap();
    kb_app::run(
        AppRequest::Apply {
            operation_id: operation_id(&plan),
        },
        &context,
    )
    .unwrap();

    let link = vault.join(".claude/skills/knowledge-brain");
    assert!(
        fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert!(link.join("SKILL.md").is_file());
    assert!(
        temp.path()
            .join("config/skills/knowledge-brain/SKILL.md")
            .is_file()
    );

    let uninstall = kb_app::run(
        AppRequest::Skills(SkillRequest::Uninstall {
            vault: Some(vault.to_string_lossy().into_owned()),
            host: Some(SkillHost::ClaudeCode),
            scope: SkillScope::Vault,
        }),
        &context,
    )
    .unwrap();
    kb_app::run(
        AppRequest::Apply {
            operation_id: operation_id(&uninstall),
        },
        &context,
    )
    .unwrap();
    assert!(fs::symlink_metadata(link).is_err());
}

fn context(base: &Path) -> AppContext {
    let environment = BTreeMap::from([
        (
            "KB_CONFIG_DIR".to_owned(),
            base.join("config").to_string_lossy().into_owned(),
        ),
        (
            "KB_STATE_DIR".to_owned(),
            base.join("state").to_string_lossy().into_owned(),
        ),
        (
            "KB_CACHE_DIR".to_owned(),
            base.join("cache").to_string_lossy().into_owned(),
        ),
        (
            "KB_AGENT_HOME".to_owned(),
            base.join("home").to_string_lossy().into_owned(),
        ),
        (
            "KB_AGENT_CONFIG_DIR".to_owned(),
            base.join("agent-config").to_string_lossy().into_owned(),
        ),
    ]);
    AppContext::new(environment, base.to_path_buf())
}

fn operation_id(value: &Value) -> OperationId {
    value["operation_id"].as_str().unwrap().parse().unwrap()
}
