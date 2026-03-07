//! Integration tests for transcript recording and the `context` builtin.
//!
//! These tests exercise the interaction between `clank-core`'s recording call
//! sites and the `context` builtin, using the public API only.
//!
//! ## Isolation
//!
//! The transcript is a process-global shared across all tests in this binary.
//! `cargo test` runs tests in parallel by default. Every test here acquires
//! `TEST_LOCK` before touching the transcript, serialising all transcript
//! access and ensuring each test sees only what it recorded.
//!
//! `TEST_LOCK` is a `tokio::sync::Mutex` so the guard can be held across
//! `.await` points without triggering `clippy::await_holding_lock`.

use std::io::Cursor;

use clank_core::{default_options, interactive_options, run, run_interactive, Shell};
use tokio::sync::Mutex;

// ---------------------------------------------------------------------------
// Test serialisation lock
// ---------------------------------------------------------------------------

static TEST_LOCK: Mutex<()> = Mutex::const_new(());

/// Acquire the test lock and clear the transcript, returning the guard.
/// Hold the guard for the duration of the test to prevent races.
async fn setup() -> tokio::sync::MutexGuard<'static, ()> {
    let guard = TEST_LOCK.lock().await;
    clank_transcript::global()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clear();
    guard
}

fn transcript_entries() -> Vec<String> {
    clank_transcript::global()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .entries()
        .map(str::to_owned)
        .collect()
}

// ---------------------------------------------------------------------------
// Recording via run()
// ---------------------------------------------------------------------------

#[tokio::test]
async fn run_records_command_text() {
    let _guard = setup().await;
    run("echo hello").await.expect("run should not error");
    let entries = transcript_entries();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0], "echo hello");
}

#[tokio::test]
async fn run_multiple_commands_each_recorded_separately() {
    let _guard = setup().await;
    run("true").await.expect("run should not error");
    run("false").await.expect("run should not error");
    let entries = transcript_entries();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0], "true");
    assert_eq!(entries[1], "false");
}

// ---------------------------------------------------------------------------
// context show via run()
// ---------------------------------------------------------------------------

#[tokio::test]
async fn context_show_exits_zero() {
    let _guard = setup().await;
    let code = run("context show").await.expect("run should not error");
    assert_eq!(code, 0);
}

#[tokio::test]
async fn context_clear_exits_zero() {
    let _guard = setup().await;
    let code = run("context clear").await.expect("run should not error");
    assert_eq!(code, 0);
}

#[tokio::test]
async fn context_clear_empties_transcript() {
    let _guard = setup().await;
    run("echo a").await.expect("run should not error");
    // "context clear" is recorded before it executes; after it runs the
    // transcript is empty. Verify via the global directly.
    run("context clear").await.expect("run should not error");
    let entries = transcript_entries();
    assert!(
        entries.is_empty(),
        "transcript should be empty after context clear; got {entries:?}"
    );
}

#[tokio::test]
async fn context_trim_exits_zero() {
    let _guard = setup().await;
    run("echo a").await.expect("run should not error");
    let code = run("context trim 1").await.expect("run should not error");
    assert_eq!(code, 0);
}

#[tokio::test]
async fn context_trim_removes_oldest_entry() {
    let _guard = setup().await;
    run("echo first").await.expect("run should not error");
    run("echo second").await.expect("run should not error");
    // trim 1 removes the oldest; "echo second" should remain, plus the trim
    // command itself which is recorded before it runs.
    run("context trim 1").await.expect("run should not error");
    let entries = transcript_entries();
    assert!(
        !entries.iter().any(|e| e == "echo first"),
        "echo first should have been trimmed; got {entries:?}"
    );
    assert!(
        entries.iter().any(|e| e == "echo second"),
        "echo second should still be present; got {entries:?}"
    );
}

#[tokio::test]
async fn context_trim_zero_is_noop() {
    let _guard = setup().await;
    run("echo a").await.expect("run should not error");
    run("context trim 0").await.expect("run should not error");
    let entries = transcript_entries();
    assert!(
        entries.iter().any(|e| e == "echo a"),
        "echo a should still be present after trim 0; got {entries:?}"
    );
}

#[tokio::test]
async fn context_trim_invalid_arg_exits_2() {
    let _guard = setup().await;
    let code = run("context trim notanumber")
        .await
        .expect("run should not error");
    assert_eq!(code, 2);
}

#[tokio::test]
async fn context_unknown_subcommand_exits_2() {
    let _guard = setup().await;
    let code = run("context bogus").await.expect("run should not error");
    assert_eq!(code, 2);
}

#[tokio::test]
async fn context_no_subcommand_exits_2() {
    let _guard = setup().await;
    let code = run("context").await.expect("run should not error");
    assert_eq!(code, 2);
}

// ---------------------------------------------------------------------------
// Recording via run_interactive()
// ---------------------------------------------------------------------------

#[tokio::test]
async fn run_interactive_records_each_line_separately() {
    let _guard = setup().await;
    let mut shell = Shell::new(interactive_options())
        .await
        .expect("shell creation should not error");
    let input = b"echo a\necho b\n" as &[u8];
    run_interactive(&mut shell, input, std::io::sink())
        .await
        .expect("run_interactive should not error");
    let entries = transcript_entries();
    assert!(
        entries.iter().any(|e| e == "echo a"),
        "echo a should be recorded; got {entries:?}"
    );
    assert!(
        entries.iter().any(|e| e == "echo b"),
        "echo b should be recorded; got {entries:?}"
    );
}

#[tokio::test]
async fn run_interactive_context_clear_empties_transcript() {
    let _guard = setup().await;
    let mut shell = Shell::new(interactive_options())
        .await
        .expect("shell creation should not error");
    // Record some entries then clear.
    let input = b"echo before\ncontext clear\n" as &[u8];
    run_interactive(&mut shell, input, std::io::sink())
        .await
        .expect("run_interactive should not error");
    let entries = transcript_entries();
    assert!(
        entries.is_empty(),
        "transcript should be empty after context clear in interactive mode; got {entries:?}"
    );
}

#[tokio::test]
async fn run_and_run_interactive_share_the_same_transcript() {
    let _guard = setup().await;
    // Record via run().
    run("echo from-run").await.expect("run should not error");
    // Record via run_interactive().
    let mut shell = Shell::new(default_options())
        .await
        .expect("shell creation should not error");
    let input = Cursor::new(b"echo from-interactive\n");
    run_interactive(&mut shell, input, std::io::sink())
        .await
        .expect("run_interactive should not error");
    let entries = transcript_entries();
    assert!(
        entries.iter().any(|e| e == "echo from-run"),
        "run() entry should be in shared transcript; got {entries:?}"
    );
    assert!(
        entries.iter().any(|e| e == "echo from-interactive"),
        "run_interactive() entry should be in shared transcript; got {entries:?}"
    );
}
