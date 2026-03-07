use assert_cmd::Command;
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;

fn clank() -> Command {
    Command::new(assert_cmd::cargo::cargo_bin!("clank"))
}

// Acceptance test 3: echo hello prints hello
#[test]
fn echo_hello() {
    clank()
        .write_stdin("echo hello\n")
        .assert()
        .success()
        .stdout(contains("hello"));
}

// Acceptance test 4: ls produces output (non-empty stdout)
#[test]
fn ls_produces_output() {
    clank()
        .write_stdin("ls\n")
        .assert()
        .success()
        .stdout(predicates::str::is_empty().not());
}

// Acceptance test 7: exit code 0 after successful command
#[test]
fn exit_code_zero_after_success() {
    clank()
        .write_stdin("true\necho $?\n")
        .assert()
        .success()
        .stdout(contains("0"));
}

// Acceptance test 8: exit code 1 after failing command
#[test]
fn exit_code_one_after_failure() {
    clank()
        .write_stdin("false\necho $?\n")
        .assert()
        .success()
        .stdout(contains("1"));
}
