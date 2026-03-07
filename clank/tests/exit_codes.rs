//! Integration tests for exit code propagation.
//! Verifies that $? correctly reflects the exit status of the last command,
//! including success, failure, and pipelines.

mod common;

use common::run_script;
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;

// --- Basic exit codes ---

/// $? is 0 after a successful command.
#[test]
fn dollar_question_is_zero_after_success() {
    run_script("true\necho $?").success().stdout(contains("0"));
}

/// $? is 1 after `false`.
#[test]
fn dollar_question_is_one_after_false() {
    run_script("false\necho $?").success().stdout(contains("1"));
}

/// $? is 0 after a successful echo.
#[test]
fn dollar_question_is_zero_after_echo() {
    run_script("echo hello\necho $?")
        .success()
        .stdout(contains("0"));
}

/// $? updates on each command — previous value does not bleed through.
#[test]
fn dollar_question_updates_each_command() {
    run_script("false\ntrue\necho $?")
        .success()
        .stdout(contains("0"));
}

/// $? is non-zero after a command that exits with failure.
#[test]
fn dollar_question_nonzero_after_failure() {
    run_script("false\necho $?")
        .success()
        .stdout(predicates::str::contains("0").not().or(contains("1")));
}

// --- Variable assignment as exit code source ---

/// Assigning a variable succeeds ($? = 0).
#[test]
fn variable_assignment_exits_zero() {
    run_script("x=hello\necho $?")
        .success()
        .stdout(contains("0"));
}

// --- Clank's own exit code ---

/// clank itself exits with code 0 regardless of the last command's exit code.
/// The shell process exit code is separate from $?.
#[test]
fn clank_exits_zero_even_after_failing_command() {
    run_script("false").success().code(0);
}
