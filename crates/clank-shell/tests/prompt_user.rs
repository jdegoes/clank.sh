/// Level 2 integration tests for `prompt-user`.
///
/// The `prompt-user` builtin reads from the *real* terminal stdin (not from the
/// process's `ProcessIo` stdin) so that it always surfaces a prompt to the human
/// regardless of piped input. In a test environment we can simulate EOF / Ctrl-C
/// by redirecting stdin from `/dev/null` using the shell's redirection syntax.
use std::sync::{Arc, RwLock};

use clank_http::MockHttpClient;
use clank_shell::{ClankShell, Transcript};

async fn make_shell() -> (ClankShell, Arc<RwLock<Transcript>>) {
    let transcript = Arc::new(RwLock::new(Transcript::default()));
    let http = Arc::new(MockHttpClient::new(vec![]));
    let shell = ClankShell::with_http(Arc::clone(&transcript), http)
        .await
        .expect("failed to create shell");
    (shell, transcript)
}

// ---------------------------------------------------------------------------
// Ctrl-C / EOF contract
// ---------------------------------------------------------------------------

/// When stdin is redirected from /dev/null, `read_line` returns `Ok(0)` (EOF)
/// immediately. `prompt-user` must return exit code 130 — the standard exit code
/// for "interrupted by signal" (Ctrl-C convention).
///
/// This is the same code path exercised when a user presses Ctrl-C at a real
/// terminal: `read_line` / `read_password` returns an error or EOF, and
/// `read_response` returns `Err(130)`.
#[tokio::test]
async fn test_prompt_user_eof_exits_130() {
    let (mut shell, _) = make_shell().await;
    // Redirect real stdin from /dev/null — this triggers immediate EOF.
    let code = shell
        .run_line("prompt-user 'what is your name?' </dev/null")
        .await;
    assert_eq!(
        code, 130,
        "prompt-user with EOF stdin must exit 130 (Ctrl-C convention)"
    );
}

/// Same contract with --confirm flag.
#[tokio::test]
async fn test_prompt_user_confirm_eof_exits_130() {
    let (mut shell, _) = make_shell().await;
    let code = shell
        .run_line("prompt-user --confirm 'proceed?' </dev/null")
        .await;
    assert_eq!(
        code, 130,
        "prompt-user --confirm with EOF stdin must exit 130"
    );
}

/// Same contract with --choices flag.
#[tokio::test]
async fn test_prompt_user_choices_eof_exits_130() {
    let (mut shell, _) = make_shell().await;
    let code = shell
        .run_line("prompt-user --choices yes,no,maybe 'which option?' </dev/null")
        .await;
    assert_eq!(
        code, 130,
        "prompt-user --choices with EOF stdin must exit 130"
    );
}

// ---------------------------------------------------------------------------
// Exit code propagation — 130 is not special to the shell loop
// ---------------------------------------------------------------------------

/// Exit code 130 from `prompt-user` must set `$?` to 130 and allow subsequent
/// commands to run. The shell loop must not abort on 130.
#[tokio::test]
async fn test_prompt_user_eof_sets_dollar_question() {
    let (mut shell, _) = make_shell().await;
    shell.run_line("prompt-user 'question' </dev/null").await;
    // $? should now be 130; verify by capturing it.
    let code = shell.run_line("test $? -eq 130").await;
    assert_eq!(
        code, 0,
        "$? must be 130 after prompt-user EOF; `test $? -eq 130` must succeed"
    );
}

/// A command after a prompt-user Ctrl-C must still run — the shell must not exit.
#[tokio::test]
async fn test_shell_continues_after_prompt_user_eof() {
    let (mut shell, _) = make_shell().await;
    shell.run_line("prompt-user 'question' </dev/null").await;
    // If the shell had exited, this would not run and the test would be vacuously
    // true — but the shell instance would have panicked or returned early.
    let code = shell.run_line("echo still_running").await;
    assert_eq!(
        code, 0,
        "shell must continue running after prompt-user exits 130"
    );
}

// ---------------------------------------------------------------------------
// Success path
// ---------------------------------------------------------------------------

/// prompt-user with --confirm and piped stdin providing a valid choice exits 0.
/// Note: prompt-user reads the *real* terminal stdin for its response, not the
/// piped ProcessIo stdin. The piped stdin is consumed as Markdown context before
/// the prompt is shown. We can only test the success path by ensuring the
/// `</dev/null` redirect is NOT applied and the shell's stdin provides a response.
///
/// Since we cannot inject terminal responses in tests, we verify the success path
/// indirectly: a `prompt-user` call in a script that evaluates to 0 only when the
/// result is "yes" should exit non-zero when stdin is /dev/null (EOF → 130), and
/// the overall logic must be sound.
///
/// This test verifies the exit code contract of the no-input path to ensure 130
/// is the signal for failure and 0 is reserved for success.
#[tokio::test]
async fn test_prompt_user_only_exits_0_on_valid_response() {
    let (mut shell, _) = make_shell().await;
    // With /dev/null (EOF), the exit code is 130, not 0.
    let code = shell
        .run_line("prompt-user --confirm 'approve?' </dev/null")
        .await;
    assert_ne!(
        code, 0,
        "prompt-user must not exit 0 on EOF — only exits 0 on valid response"
    );
    assert_eq!(code, 130, "EOF must produce exit 130");
}
