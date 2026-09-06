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
        .stdout(predicate::str::contains("Usage: kb"));
}
