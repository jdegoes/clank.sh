//! Integration tests for REPL loop behaviour.
//! These tests verify how clank handles input at the loop level:
//! prompt placement, empty lines, EOF, exit, and basic execution.

mod common;

use common::{clank, run_script};
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;

// --- Basic execution ---

/// The most fundamental test: a single echo command produces output on stdout.
#[test]
fn echo_hello_prints_to_stdout() {
    run_script("echo hello").success().stdout(contains("hello"));
}

/// Multiple commands on separate lines are all executed in sequence.
#[test]
fn multiple_commands_execute_in_sequence() {
    run_script("echo first\necho second")
        .success()
        .stdout(contains("first"))
        .stdout(contains("second"));
}

/// Empty lines are silently skipped without error.
#[test]
fn empty_lines_are_ignored() {
    run_script("\n\n\necho after_blanks\n\n")
        .success()
        .stdout(contains("after_blanks"));
}

/// Lines containing only whitespace are silently skipped.
#[test]
fn whitespace_only_lines_are_ignored() {
    run_script("   \n\t\necho ok")
        .success()
        .stdout(contains("ok"));
}

// --- Prompt placement ---

/// The prompt ("$ ") is written to stderr, not stdout.
/// This ensures stdout is clean for programmatic use and test assertions.
#[test]
fn prompt_is_on_stderr_not_stdout() {
    clank()
        .write_stdin("echo hi\n")
        .assert()
        .success()
        .stdout(predicates::str::contains("$ ").not()) // prompt must NOT appear on stdout
        .stderr(contains("$ ")); // prompt MUST appear on stderr
}

// --- Exit and EOF ---

/// The `exit` command terminates the shell cleanly with exit code 0.
#[test]
fn exit_command_terminates_with_code_zero() {
    clank().write_stdin("exit\n").assert().success().code(0);
}

/// EOF (Ctrl-D, i.e. closing stdin) terminates the shell cleanly.
#[test]
fn eof_terminates_cleanly() {
    clank()
        .write_stdin("") // empty stdin = immediate EOF
        .assert()
        .success()
        .code(0);
}

/// Commands after `exit` are not executed.
#[test]
fn commands_after_exit_are_not_run() {
    run_script("exit\necho should_not_appear")
        .success()
        .stdout(predicates::str::contains("should_not_appear").not());
}
