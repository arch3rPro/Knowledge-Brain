use assert_cmd::Command;

const COMMAND_REFERENCE: &str = include_str!("../../../docs/reference/commands.md");

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
