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
        .stdout(predicates::str::contains("clank$").not()) // prompt must NOT appear on stdout
        .stderr(contains("clank$")); // prompt MUST appear on stderr
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

// --- context commands ---

/// context show prints previously run commands to stdout.
#[test]
fn context_show_prints_transcript() {
    run_script("echo hello\ncontext show")
        .success()
        .stdout(contains("echo hello"));
}

/// context show output is NOT re-recorded — running it twice produces
/// the same transcript content (not growing).
#[test]
fn context_show_does_not_record_itself() {
    run_script("echo hi\ncontext show\ncontext show")
        .success()
        // The transcript shown both times should be identical —
        // context show must not add itself as an entry.
        // We verify by checking the transcript only contains "echo hi" once.
        .stdout(predicates::str::contains("context show").not());
}

/// context clear empties the transcript — subsequent context show prints nothing.
#[test]
fn context_clear_empties_transcript() {
    run_script("echo hello\ncontext clear\ncontext show")
        .success()
        .stdout(predicates::str::contains("echo hello").not());
}

/// context trim drops the oldest entries — after trimming the first command's
/// entries, context show no longer contains that command in the transcript.
#[test]
fn context_trim_drops_oldest_entries() {
    // Run two commands, trim 2 entries (Command+Output for "first"), then show.
    // The transcript shown by context show should contain "echo second" but
    // not "echo first" as a recorded command.
    run_script("echo first\necho second\ncontext trim 2\ncontext show")
        .success()
        .stdout(contains("echo second")) // second command still in transcript
        .stdout(contains("$ echo first").not()); // first command entry removed
}

// --- model commands ---

/// model list with no providers configured prints a helpful message.
#[test]
fn model_list_no_providers() {
    let tmp = tempfile::tempdir().unwrap();
    clank()
        .env("HOME", tmp.path())
        .write_stdin("model list\n")
        .assert()
        .success()
        .stdout(contains("No providers configured."));
}

/// model add then model list shows the new provider.
#[test]
fn model_add_then_list() {
    let tmp = tempfile::tempdir().unwrap();
    clank()
        .env("HOME", tmp.path())
        .write_stdin("model add anthropic --key sk-test-123\nmodel list\n")
        .assert()
        .success()
        .stdout(contains("anthropic"))
        .stdout(contains("sk-test-123").not()); // key must be redacted
}

/// model default sets the default model shown in model list.
#[test]
fn model_default_then_list() {
    let tmp = tempfile::tempdir().unwrap();
    clank()
        .env("HOME", tmp.path())
        .write_stdin(
            "model add anthropic --key sk-test\nmodel default anthropic/claude-sonnet-4-5\nmodel list\n",
        )
        .assert()
        .success()
        .stdout(contains("anthropic/claude-sonnet-4-5"));
}
