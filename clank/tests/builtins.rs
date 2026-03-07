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
