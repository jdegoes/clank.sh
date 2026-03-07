//! Integration tests for builtin commands.
//! Each builtin gets its own section. Tests verify correctness of output,
//! side effects, and exit codes. This file will grow as more builtins are added.

mod common;

use common::run_script;
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;

// --- echo ---

#[test]
fn echo_single_word() {
    run_script("echo hello").success().stdout(contains("hello"));
}

#[test]
fn echo_multiple_words() {
    run_script("echo foo bar baz")
        .success()
        .stdout(contains("foo bar baz"));
}

#[test]
fn echo_empty_produces_newline() {
    run_script("echo").success().stdout(contains("\n"));
}

#[test]
fn echo_outputs_to_stdout_not_stderr() {
    run_script("echo hello")
        .success()
        .stdout(contains("hello"))
        .stderr(contains("hello").not());
}

// --- true / false ---

#[test]
fn true_exits_zero() {
    run_script("true\necho $?").success().stdout(contains("0"));
}

#[test]
fn false_exits_one() {
    run_script("false\necho $?").success().stdout(contains("1"));
}

// --- pwd ---

#[test]
fn pwd_outputs_a_path() {
    // pwd should print something that looks like an absolute path
    run_script("pwd").success().stdout(contains("/"));
}

#[test]
fn pwd_exits_zero() {
    run_script("pwd\necho $?").success().stdout(contains("0"));
}

// --- cd ---

#[test]
fn cd_to_valid_directory_exits_zero() {
    // /tmp always exists on macOS and Linux
    run_script("cd /tmp\necho $?")
        .success()
        .stdout(contains("0"));
}

#[test]
fn cd_updates_working_directory() {
    run_script("cd /tmp\npwd").success().stdout(contains("tmp"));
}

#[test]
fn cd_to_invalid_directory_exits_nonzero() {
    run_script("cd /this_directory_does_not_exist_clank_test\necho $?")
        .success()
        .stdout(predicates::str::contains("0").not());
}

// --- ls ---

#[test]
fn ls_produces_output() {
    run_script("ls")
        .success()
        .stdout(predicates::str::is_empty().not());
}

#[test]
fn ls_nonexistent_path_exits_nonzero() {
    run_script("ls /this_path_does_not_exist_clank_test\necho $?")
        .success()
        .stdout(predicates::str::contains("0").not());
}

// --- ls OS equivalence tests ---
//
// These tests verify that clank's internal ls produces output identical to
// the real OS ls for the same inputs. The fixture directory has known, stable
// contents committed to git.

/// Absolute path to the ls fixture directory, resolved at compile time.
fn ls_fixture() -> String {
    format!(
        "{}/tests/golden/fixtures/ls-test-dir",
        env!("CARGO_MANIFEST_DIR")
    )
}

/// Run the real OS `ls` with the given args on the fixture and return stdout.
fn os_ls(args: &[&str]) -> String {
    let output = std::process::Command::new("ls")
        .args(args)
        .output()
        .expect("failed to spawn OS ls");
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// Run clank's internal `ls` with the given args on the fixture and return stdout.
fn clank_ls(args: &str) -> String {
    let script = format!("ls {args}");
    let output = common::clank()
        .write_stdin(script)
        .output()
        .expect("failed to run clank");
    String::from_utf8_lossy(&output.stdout).into_owned()
}

#[test]
fn ls_plain_matches_os() {
    let fixture = ls_fixture();
    let expected = os_ls(&[&fixture]);
    let actual = clank_ls(&fixture);
    assert_eq!(actual, expected, "plain ls output differs from OS ls");
}

#[test]
fn ls_a_matches_os() {
    let fixture = ls_fixture();
    let expected = os_ls(&["-a", &fixture]);
    let actual = clank_ls(&format!("-a {fixture}"));
    assert_eq!(actual, expected, "ls -a output differs from OS ls -a");
}

#[test]
fn ls_recursive_matches_os() {
    let fixture = ls_fixture();
    let expected = os_ls(&["-R", &fixture]);
    let actual = clank_ls(&format!("-R {fixture}"));
    assert_eq!(actual, expected, "ls -R output differs from OS ls -R");
}
