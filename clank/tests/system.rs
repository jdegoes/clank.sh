//! System tests: multi-step realistic scenarios.
//! These tests exercise the shell as a whole, verifying that multiple
//! commands compose correctly with shared state (variables, working dir, etc.).

mod common;

use common::run_script;
use predicates::str::contains;

// --- Variable assignment and expansion ---

/// Assign a variable, then expand it with echo.
#[test]
fn scenario_variable_assign_and_expand() {
    run_script("name=world\necho hello $name")
        .success()
        .stdout(contains("hello world"));
}

/// Variable set in one command is visible in a subsequent command.
#[test]
fn scenario_variable_persists_across_commands() {
    run_script("x=42\necho $x").success().stdout(contains("42"));
}

/// Reassigning a variable updates its value.
#[test]
fn scenario_variable_reassignment() {
    run_script("x=first\nx=second\necho $x")
        .success()
        .stdout(contains("second"));
}

// --- Command sequences and exit code tracking ---

/// A sequence of commands: last exit code reflects the last one.
#[test]
fn scenario_exit_code_tracks_last_command() {
    run_script("true\nfalse\necho $?")
        .success()
        .stdout(contains("1"));
}

/// Recover from failure: run a succeeding command after a failing one.
#[test]
fn scenario_recover_after_failure() {
    run_script("false\ntrue\necho $?")
        .success()
        .stdout(contains("0"));
}

// --- Pipes ---

/// A simple pipe: echo piped through cat passes the value through.
#[test]
fn scenario_simple_pipe() {
    run_script("echo piped_value | cat")
        .success()
        .stdout(contains("piped_value"));
}

// --- Working directory ---

/// cd changes the working directory; pwd reflects it.
#[test]
fn scenario_cd_then_pwd() {
    run_script("cd /tmp\npwd").success().stdout(contains("tmp"));
}

/// Working directory state persists across commands.
#[test]
fn scenario_cd_and_list() {
    run_script("cd /tmp\nls").success();
}
