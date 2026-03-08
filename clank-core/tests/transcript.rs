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
use clank_transcript::TranscriptEntry;
use tokio::sync::Mutex;

// ---------------------------------------------------------------------------
// Test serialisation lock
// ---------------------------------------------------------------------------

static TEST_LOCK: Mutex<()> = Mutex::const_new(());

async fn setup() -> tokio::sync::MutexGuard<'static, ()> {
    let guard = TEST_LOCK.lock().await;
    clank_transcript::global()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clear();
    guard
}

// ---------------------------------------------------------------------------
// Helpers
//
// `entries()` returns (tag, text) pairs — the full transcript without
// timestamps. This makes assertions read exactly like the content:
//
//   assert_eq!(entries(), vec![("command", "echo a"), ("output", "a")]);
// ---------------------------------------------------------------------------

fn entries() -> Vec<(&'static str, String)> {
    clank_transcript::global()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .entries()
        .map(|e| (e.kind.tag(), e.kind.text().to_owned()))
        .collect()
}

// ---------------------------------------------------------------------------
// Recording via run() — Command and Output entries
// ---------------------------------------------------------------------------

#[tokio::test]
async fn run_records_command_and_output() {
    let _guard = setup().await;
    run("echo hello").await.expect("run should not error");
    assert_eq!(
        entries(),
        vec![("command", "echo hello".into()), ("output", "hello".into()),]
    );
}

#[tokio::test]
async fn run_multiple_commands_each_recorded_separately() {
    let _guard = setup().await;
    run("true").await.expect("run should not error");
    run("false").await.expect("run should not error");
    assert_eq!(
        entries(),
        vec![("command", "true".into()), ("command", "false".into()),]
    );
}

#[tokio::test]
async fn run_multi_statement_script_records_per_statement() {
    let _guard = setup().await;
    // Two statements in one script — each gets its own Command+Output pair.
    run("echo a\necho b").await.expect("run should not error");
    assert_eq!(
        entries(),
        vec![
            ("command", "echo a".into()),
            ("output", "a".into()),
            ("command", "echo b".into()),
            ("output", "b".into()),
        ]
    );
}

#[tokio::test]
async fn run_entry_has_timestamp() {
    let _guard = setup().await;
    let before = chrono::Utc::now();
    run("true").await.expect("run should not error");
    let after = chrono::Utc::now();
    let raw: Vec<TranscriptEntry> = clank_transcript::global()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .entries()
        .cloned()
        .collect();
    assert_eq!(raw.len(), 1);
    assert!(raw[0].timestamp >= before);
    assert!(raw[0].timestamp <= after);
}

#[tokio::test]
async fn silent_command_produces_no_output_entry() {
    let _guard = setup().await;
    run("true").await.expect("run should not error");
    assert_eq!(entries(), vec![("command", "true".into())]);
}

// ---------------------------------------------------------------------------
// context show output not re-recorded
//
// context show executes before its own entry is recorded. It sees the
// transcript as it was before the call. Its stdout output is suppressed from
// the Output recording path. So after `run("echo seed")` then
// `run("context show")`, the transcript contains exactly:
//   Command("echo seed"), Output("seed"), Command("context show")
// — no Output entry from show's stdout.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn context_show_output_not_re_recorded() {
    let _guard = setup().await;
    run("echo seed").await.expect("run should not error");
    run("context show").await.expect("run should not error");
    assert_eq!(
        entries(),
        vec![
            ("command", "echo seed".into()),
            ("output", "seed".into()),
            ("command", "context show".into()),
        ]
    );
}

// ---------------------------------------------------------------------------
// context show / clear / trim exit codes
// ---------------------------------------------------------------------------

#[tokio::test]
async fn context_show_exits_zero() {
    let _guard = setup().await;
    assert_eq!(run("context show").await.expect("run should not error"), 0);
}

#[tokio::test]
async fn context_clear_empties_transcript_completely() {
    let _guard = setup().await;
    run("echo a").await.expect("run should not error");
    run("context clear").await.expect("run should not error");
    // context clear is a self-erasing command: it clears the transcript and
    // does not record itself. The transcript must be completely empty afterward.
    assert_eq!(entries(), vec![]);
}

#[tokio::test]
async fn context_trim_exits_zero() {
    let _guard = setup().await;
    run("echo a").await.expect("run should not error");
    assert_eq!(
        run("context trim 1").await.expect("run should not error"),
        0
    );
}

#[tokio::test]
async fn context_trim_invalid_arg_exits_2() {
    let _guard = setup().await;
    assert_eq!(
        run("context trim notanumber")
            .await
            .expect("run should not error"),
        2
    );
}

#[tokio::test]
async fn context_unknown_subcommand_exits_2() {
    let _guard = setup().await;
    assert_eq!(run("context bogus").await.expect("run should not error"), 2);
}

#[tokio::test]
async fn context_no_subcommand_exits_2() {
    let _guard = setup().await;
    assert_eq!(run("context").await.expect("run should not error"), 2);
}

// ---------------------------------------------------------------------------
// context trim semantics
// ---------------------------------------------------------------------------

#[tokio::test]
async fn context_trim_removes_oldest_entry() {
    let _guard = setup().await;
    run("echo first").await.expect("run should not error");
    run("echo second").await.expect("run should not error");
    // Before trim: [Command("echo first"), Output("first"),
    //               Command("echo second"), Output("second")]
    // trim 1 removes the oldest entry: Command("echo first").
    // context trim is self-erasing: it does not record itself.
    run("context trim 1").await.expect("run should not error");
    assert_eq!(
        entries(),
        vec![
            ("output", "first".into()),
            ("command", "echo second".into()),
            ("output", "second".into()),
        ]
    );
}

#[tokio::test]
async fn context_trim_zero_is_noop() {
    let _guard = setup().await;
    run("echo a").await.expect("run should not error");
    run("context trim 0").await.expect("run should not error");
    // context trim is self-erasing: it does not record itself.
    assert_eq!(
        entries(),
        vec![("command", "echo a".into()), ("output", "a".into()),]
    );
}

// ---------------------------------------------------------------------------
// run_interactive recording
// ---------------------------------------------------------------------------

#[tokio::test]
async fn run_interactive_records_each_line_separately() {
    let _guard = setup().await;
    let mut shell = Shell::new(interactive_options())
        .await
        .expect("shell creation should not error");
    run_interactive(&mut shell, b"echo a\necho b\n" as &[u8], std::io::sink())
        .await
        .expect("run_interactive should not error");
    assert_eq!(
        entries(),
        vec![
            ("command", "echo a".into()),
            ("output", "a".into()),
            ("command", "echo b".into()),
            ("output", "b".into()),
        ]
    );
}

#[tokio::test]
async fn run_interactive_captures_output() {
    let _guard = setup().await;
    let mut shell = Shell::new(interactive_options())
        .await
        .expect("shell creation should not error");
    run_interactive(
        &mut shell,
        b"echo interactive-out\n" as &[u8],
        std::io::sink(),
    )
    .await
    .expect("run_interactive should not error");
    assert_eq!(
        entries(),
        vec![
            ("command", "echo interactive-out".into()),
            ("output", "interactive-out".into()),
        ]
    );
}

#[tokio::test]
async fn run_interactive_context_clear_empties_transcript_completely() {
    let _guard = setup().await;
    let mut shell = Shell::new(interactive_options())
        .await
        .expect("shell creation should not error");
    run_interactive(
        &mut shell,
        b"echo before\ncontext clear\n" as &[u8],
        std::io::sink(),
    )
    .await
    .expect("run_interactive should not error");
    // context clear is self-erasing: clears prior entries and does not record
    // itself. The transcript must be completely empty afterward.
    assert_eq!(entries(), vec![]);
}

#[tokio::test]
async fn run_and_run_interactive_share_the_same_transcript() {
    let _guard = setup().await;
    run("echo from-run").await.expect("run should not error");
    let mut shell = Shell::new(default_options())
        .await
        .expect("shell creation should not error");
    run_interactive(
        &mut shell,
        Cursor::new(b"echo from-interactive\n"),
        std::io::sink(),
    )
    .await
    .expect("run_interactive should not error");
    assert_eq!(
        entries(),
        vec![
            ("command", "echo from-run".into()),
            ("output", "from-run".into()),
            ("command", "echo from-interactive".into()),
            ("output", "from-interactive".into()),
        ]
    );
}

// ---------------------------------------------------------------------------
// Redaction
// ---------------------------------------------------------------------------

#[tokio::test]
async fn aws_key_in_command_is_redacted() {
    let _guard = setup().await;
    run("echo AKIA1234567890ABCDEF")
        .await
        .expect("run should not error");
    let e = entries();
    // Command entry must not contain the raw key.
    assert!(
        !e.iter().any(|(_, v)| v.contains("AKIA1234567890ABCDEF")),
        "raw AWS key should be redacted; got {e:?}"
    );
    assert!(
        e.iter().any(|(_, v)| v.contains("[REDACTED]")),
        "redaction marker should be present; got {e:?}"
    );
}

#[tokio::test]
async fn env_var_password_in_output_is_redacted() {
    let _guard = setup().await;
    // run() uses interactive mode which captures output.
    // In script mode output is not captured, so use run_interactive.
    let mut shell = Shell::new(interactive_options())
        .await
        .expect("shell creation should not error");
    run_interactive(
        &mut shell,
        b"echo DB_PASSWORD=hunter2\n" as &[u8],
        std::io::sink(),
    )
    .await
    .expect("run_interactive should not error");
    let e = entries();
    assert!(
        !e.iter().any(|(_, v)| v.contains("hunter2")),
        "password value should be redacted; got {e:?}"
    );
    assert!(
        e.iter().any(|(_, v)| v.contains("[REDACTED]")),
        "redaction marker should be present; got {e:?}"
    );
}

#[tokio::test]
async fn normal_content_is_not_redacted() {
    let _guard = setup().await;
    run("echo hello").await.expect("run should not error");
    let e = entries();
    assert!(
        e.iter().any(|(_, v)| v.contains("hello")),
        "normal content should not be redacted; got {e:?}"
    );
    assert!(
        !e.iter().any(|(_, v)| v.contains("[REDACTED]")),
        "redaction marker should not appear for normal content; got {e:?}"
    );
}
