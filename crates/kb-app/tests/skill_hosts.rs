use kb_app::{AgentRoots, detect_skill_hosts, resolve_skill_host, skill_target};
use kb_core::{ErrorCode, SkillHost, SkillScope};

#[test]
fn every_host_has_portable_vault_and_user_targets() {
    let temp = tempfile::tempdir().unwrap();
    let vault = temp.path().join("vault");
    let roots = AgentRoots::new(temp.path().join("home"), temp.path().join("config"));
    let cases = [
        (SkillHost::Codex, ".agents", ".codex"),
        (SkillHost::ClaudeCode, ".claude", ".claude"),
        (SkillHost::GeminiCli, ".gemini", ".gemini"),
        (SkillHost::OpenCode, ".opencode", "opencode"),
    ];

    for (host, vault_parent, user_parent) in cases {
        let local = skill_target(&vault, &roots, host, SkillScope::Vault).unwrap();
        assert_eq!(local.skills_root, vault.join(vault_parent).join("skills"));
        assert_eq!(
            local.legacy_skill_dir,
            local.skills_root.join("knowledge-brain")
        );
        assert!(local.bridge_file.starts_with(&vault));

        let user = skill_target(&vault, &roots, host, SkillScope::User).unwrap();
        assert!(user.skills_root.ends_with("skills"));
        assert!(user.skills_root.to_string_lossy().contains(user_parent));
        assert!(!user.skills_root.starts_with(&vault));
        assert_eq!(
            user.legacy_skill_dir,
            user.skills_root.join("knowledge-brain")
        );
    }
}

#[test]
fn detection_is_explicit_about_ambiguous_shared_rule_files() {
    let temp = tempfile::tempdir().unwrap();
    let vault = temp.path();
    std::fs::write(vault.join("AGENTS.md"), "existing rules\n").unwrap();

    let detected = detect_skill_hosts(vault).unwrap();
    assert_eq!(detected.len(), 2);
    assert!(detected.iter().any(|item| item.host == SkillHost::Codex));
    assert!(detected.iter().any(|item| item.host == SkillHost::OpenCode));

    let error = resolve_skill_host(None, &detected).unwrap_err();
    assert_eq!(error.code, ErrorCode::InvalidConfig);
    assert!(error.message.contains("ambiguous"));
    assert_eq!(
        resolve_skill_host(Some(SkillHost::Codex), &detected).unwrap(),
        SkillHost::Codex
    );
}

#[test]
fn dedicated_host_directory_is_unambiguous() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir(temp.path().join(".gemini")).unwrap();
    let detected = detect_skill_hosts(temp.path()).unwrap();
    assert_eq!(
        resolve_skill_host(None, &detected).unwrap(),
        SkillHost::GeminiCli
    );
}
