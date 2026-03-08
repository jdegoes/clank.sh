//! Integration tests for `run_interactive` REPL behaviour.
//!
//! These tests focus on the aspects most at risk of silent regression:
//!
//! - **External command execution** — builtins (`true`/`false`) never touch the
//!   subprocess spawning path. Any regression that causes external commands to
//!   hang (e.g. process-group / tcsetpgrp mismanagement) is caught here because
//!   `#[tokio::test]` will time out rather than pass.
//!
//! - **Shell state persistence** — variables and working directory set in one
//!   command must be visible in the next, confirming that the same `Shell`
//!   instance is reused across the loop iterations.
//!
//! - **Prompt output** — the `$ ` prompt must be written to the `output` writer
//!   (not stdout) once per non-empty line, including before the first command.
//!
//! - **Empty line skipping** — blank lines must not execute or change state.
//!
//! - **`$?` tracking** — `$?` must reflect the exit code of the most recently
//!   completed command throughout the session.

use std::path::PathBuf;

use clank_core::{interactive_options, run_interactive, Shell};

// ---------------------------------------------------------------------------
// Helper
// ---------------------------------------------------------------------------

async fn interactive_shell() -> Shell {
    Shell::new(interactive_options())
        .await
        .expect("Shell::new should not error")
}

/// Create a temporary directory containing a fixed set of files and return
/// its path. The caller is responsible for cleanup.
fn make_test_dir() -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let id = COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("clank-interactive-test-{pid}-{id}"));
    std::fs::create_dir_all(&dir).expect("failed to create test dir");
    std::fs::write(dir.join("alpha.txt"), "a").expect("failed to write alpha.txt");
    std::fs::write(dir.join("beta.txt"), "b").expect("failed to write beta.txt");
    dir
}

// ---------------------------------------------------------------------------
// External command execution — the core regression guard
// ---------------------------------------------------------------------------

/// `ls` is a real external process. If process-group management is broken it
/// hangs indefinitely; this test times out rather than passing.
#[tokio::test]
async fn external_command_ls_completes_and_exits_zero() {
    let dir = make_test_dir();
    let mut shell = interactive_shell().await;
    let input = format!("ls {}\n", dir.display());
    let code = run_interactive(&mut shell, input.as_bytes(), std::io::sink())
        .await
        .expect("run_interactive should not error");
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(code, 0, "ls on a controlled directory should exit 0");
}

/// Multiple distinct external commands in sequence must all complete.
#[tokio::test]
async fn multiple_external_commands_all_complete() {
    let dir = make_test_dir();
    let mut shell = interactive_shell().await;
    // ls, pwd — both are real subprocesses; pwd is last so its exit code is returned.
    let input = format!("ls {dir}\npwd\n", dir = dir.display());
    let code = run_interactive(&mut shell, input.as_bytes(), std::io::sink())
        .await
        .expect("run_interactive should not error");
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(code, 0, "pwd should be the last command and exit 0");
}

/// A failing external command must return its actual exit code, not hang.
#[tokio::test]
async fn external_command_failure_returns_nonzero() {
    let mut shell = interactive_shell().await;
    let input = b"ls /nonexistent-clank-test-path\n" as &[u8];
    let code = run_interactive(&mut shell, input, std::io::sink())
        .await
        .expect("run_interactive should not error");
    assert_ne!(code, 0, "ls on a nonexistent path should exit non-zero");
}

/// External command followed by a builtin: both must complete.
#[tokio::test]
async fn external_command_followed_by_builtin_both_complete() {
    let dir = make_test_dir();
    let mut shell = interactive_shell().await;
    let input = format!("ls {}\ntrue\n", dir.display());
    let code = run_interactive(&mut shell, input.as_bytes(), std::io::sink())
        .await
        .expect("run_interactive should not error");
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(code, 0);
}

// ---------------------------------------------------------------------------
// Shell state persistence across loop iterations
// ---------------------------------------------------------------------------

/// A variable set in one line must be readable in the next.
#[tokio::test]
async fn variable_set_in_one_line_visible_in_next() {
    let mut shell = interactive_shell().await;
    // If state were reset between iterations, $X would be empty and `exit`
    // would exit with code 0, not 42.
    let input = b"X=42\nexit $X\n" as &[u8];
    let code = run_interactive(&mut shell, input, std::io::sink())
        .await
        .expect("run_interactive should not error");
    assert_eq!(code, 42, "variable set in previous line should be visible");
}

/// `cd` changes the working directory; a subsequent `pwd`-based check should
/// reflect the new location.
#[tokio::test]
async fn cd_persists_working_directory_across_lines() {
    let dir = make_test_dir();
    let out_path = dir.join("pwd-out.txt");
    let input = format!(
        "cd {dir}\npwd > {out}\n",
        dir = dir.display(),
        out = out_path.display()
    );

    let mut shell = interactive_shell().await;
    let code = run_interactive(&mut shell, input.as_bytes(), std::io::sink())
        .await
        .expect("run_interactive should not error");
    assert_eq!(code, 0);

    let written =
        std::fs::read_to_string(&out_path).expect("pwd output file should have been written");
    let written = written.trim();

    // Resolve symlinks so macOS /private/tmp == /tmp comparisons pass.
    let written_resolved =
        std::fs::canonicalize(written).unwrap_or_else(|_| PathBuf::from(written));
    let dir_resolved = std::fs::canonicalize(&dir).unwrap_or_else(|_| dir.clone());
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(
        written_resolved, dir_resolved,
        "pwd after cd should print the new directory"
    );
}

// ---------------------------------------------------------------------------
// $? tracking
// ---------------------------------------------------------------------------

/// `$?` must reflect the exit code of the immediately preceding command.
#[tokio::test]
async fn dollar_question_reflects_last_exit_code() {
    let mut shell = interactive_shell().await;
    let input = b"false\nexit $?\n" as &[u8];
    let code = run_interactive(&mut shell, input, std::io::sink())
        .await
        .expect("run_interactive should not error");
    assert_ne!(code, 0, "$? after false should be non-zero");
}

#[tokio::test]
async fn dollar_question_resets_after_successful_command() {
    let mut shell = interactive_shell().await;
    let input = b"false\ntrue\nexit $?\n" as &[u8];
    let code = run_interactive(&mut shell, input, std::io::sink())
        .await
        .expect("run_interactive should not error");
    assert_eq!(code, 0, "$? after true should be 0");
}

// ---------------------------------------------------------------------------
// Prompt output
// ---------------------------------------------------------------------------

/// The prompt `$ ` must be written before each `read_line` call, including
/// the final one that discovers EOF. Two commands followed by EOF → three
/// prompts (one per read attempt).
#[tokio::test]
async fn prompt_written_before_each_command() {
    let mut shell = interactive_shell().await;
    let mut prompt_output = Vec::new();
    run_interactive(&mut shell, b"true\ntrue\n" as &[u8], &mut prompt_output)
        .await
        .expect("run_interactive should not error");
    let s = String::from_utf8(prompt_output).expect("prompt output should be valid UTF-8");
    assert_eq!(
        s, "$ $ $ ",
        "expected three prompts: two commands plus EOF probe"
    );
}

/// A prompt is written even before the first command.
#[tokio::test]
async fn prompt_written_before_first_command() {
    let mut shell = interactive_shell().await;
    let mut prompt_output = Vec::new();
    run_interactive(&mut shell, b"true\n" as &[u8], &mut prompt_output)
        .await
        .expect("run_interactive should not error");
    let s = String::from_utf8(prompt_output).expect("prompt output should be valid UTF-8");
    assert!(
        s.starts_with("$ "),
        "prompt should appear before first command"
    );
}

/// On EOF with no commands, exactly one prompt is written before EOF is detected.
#[tokio::test]
async fn prompt_written_on_immediate_eof() {
    let mut shell = interactive_shell().await;
    let mut prompt_output = Vec::new();
    run_interactive(&mut shell, b"" as &[u8], &mut prompt_output)
        .await
        .expect("run_interactive should not error");
    let s = String::from_utf8(prompt_output).expect("prompt output should be valid UTF-8");
    assert_eq!(
        s, "$ ",
        "one prompt should be written before EOF is detected"
    );
}

// ---------------------------------------------------------------------------
// Empty line skipping
// ---------------------------------------------------------------------------

/// Blank lines must not affect the exit code or count as commands.
#[tokio::test]
async fn empty_lines_are_skipped() {
    let mut shell = interactive_shell().await;
    // false, then two blank lines, then true — last real command is true → 0.
    let input = b"false\n\n\ntrue\n" as &[u8];
    let code = run_interactive(&mut shell, input, std::io::sink())
        .await
        .expect("run_interactive should not error");
    assert_eq!(code, 0, "blank lines should not change exit code");
}

/// Blank lines generate prompts (printed before the empty-line check) but
/// must not execute as commands.
#[tokio::test]
async fn empty_lines_do_not_generate_extra_prompts() {
    let mut shell = interactive_shell().await;
    let mut prompt_output = Vec::new();
    // Two blank lines then one real command then EOF = 4 read_line calls → 4 prompts.
    run_interactive(&mut shell, b"\n\ntrue\n" as &[u8], &mut prompt_output)
        .await
        .expect("run_interactive should not error");
    let s = String::from_utf8(prompt_output).unwrap();
    assert_eq!(
        s, "$ $ $ $ ",
        "expected four prompts: two blanks + command + EOF probe"
    );
}

// ---------------------------------------------------------------------------
// Pipeline and multi-command lines
// ---------------------------------------------------------------------------

/// A pipeline of external commands must complete without hanging.
#[tokio::test]
async fn external_pipeline_completes() {
    let dir = make_test_dir();
    let mut shell = interactive_shell().await;
    let input = format!("ls {} | cat\n", dir.display());
    let code = run_interactive(&mut shell, input.as_bytes(), std::io::sink())
        .await
        .expect("run_interactive should not error");
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(code, 0, "ls <dir> | cat should exit 0");
}

/// Semicolon-separated commands on one line must all execute.
#[tokio::test]
async fn semicolon_separated_commands_all_execute() {
    let mut shell = interactive_shell().await;
    // If any assignment were skipped, $X would not be 3 and exit code would differ.
    let input = b"X=1; X=2; X=3\nexit $X\n" as &[u8];
    let code = run_interactive(&mut shell, input, std::io::sink())
        .await
        .expect("run_interactive should not error");
    assert_eq!(code, 3, "all semicolon-separated assignments should run");
}
