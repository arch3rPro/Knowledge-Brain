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
        .stdout(predicate::str::contains("lint"));
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
