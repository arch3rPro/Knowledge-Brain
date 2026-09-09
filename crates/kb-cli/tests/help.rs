use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn help_identifies_the_portable_cli() {
    Command::cargo_bin("kb")
        .unwrap()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Knowledge-Brain"))
        .stdout(predicate::str::contains("Usage: kb"))
        .stdout(predicate::str::contains("lint"))
        .stdout(predicate::str::contains("maintain"));
}

#[test]
fn lint_help_explains_the_strict_exit_policy() {
    Command::cargo_bin("kb")
        .unwrap()
        .args(["lint", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--strict"))
        .stdout(predicate::str::contains("non-zero"));
}

#[test]
fn skills_help_lists_native_host_ids() {
    Command::cargo_bin("kb")
        .unwrap()
        .args(["skills", "install", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("openclaw"))
        .stdout(predicate::str::contains("hermes"))
        .stdout(predicate::str::contains("dsh"))
        .stdout(predicate::str::contains("pi"));
}

#[test]
fn mcp_help_exposes_both_transports_and_network_policy_options() {
    Command::cargo_bin("kb")
        .unwrap()
        .args(["mcp", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("stdio"))
        .stdout(predicate::str::contains("streamable-http"))
        .stdout(predicate::str::contains("--token-file"))
        .stdout(predicate::str::contains("--allow-origin"));
}
