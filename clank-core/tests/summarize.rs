//! Integration tests for `context summarize` transcript-recording semantics.
//!
//! These tests verify the central invariant stated in the README:
//!
//! > `context show` and `context summarize` are transcript-inspection commands:
//! > their output is written to stdout but is **not recorded back into the
//! > transcript**, regardless of whether that output reaches the terminal.
//!
//! Testing `context summarize` end-to-end requires an HTTP server because the
//! command calls the configured provider.  A minimal in-process mock server
//! handles `POST /api/chat` and returns a canned Ollama-format response so no
//! real LLM or network access is needed.
//!
//! ## Isolation
//!
//! `HOME` is a process-global env var and tests in this binary run in parallel.
//! All tests here acquire `TEST_LOCK` before touching the transcript or `HOME`,
//! serialising all access so each test sees only the state it recorded.
//!
//! Each test writes its own `ask.toml` under a unique temp dir and sets `HOME`
//! to that dir under the lock.  The original `HOME` is restored before the
//! guard is dropped.

use std::io::Cursor;
use std::path::PathBuf;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::Mutex;

use clank_core::{default_options, run, run_interactive, Shell};

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
// Transcript helper — identical pattern to transcript.rs
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
// Mock Ollama server
//
// A minimal HTTP/1.1 server that responds to every POST with a canned Ollama
// chat completion JSON body.  Driven by the tokio runtime already running the
// test — no extra threads or processes.
// ---------------------------------------------------------------------------

/// Spawn a mock Ollama server that returns `summary_text` as the assistant
/// message content.  Returns the bound port and a shutdown sender; dropping
/// the sender causes the server task to exit.
async fn spawn_mock_ollama(summary_text: &'static str) -> (u16, tokio::sync::oneshot::Sender<()>) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("mock server should bind");
    let port = listener
        .local_addr()
        .expect("mock server should have a local addr")
        .port();

    let (shutdown_tx, mut shutdown_rx) = tokio::sync::oneshot::channel::<()>();

    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = &mut shutdown_rx => break,
                accepted = listener.accept() => {
                    let (mut stream, _) = accepted.expect("accept should not fail");
                    let mut buf = vec![0u8; 4096];
                    let _ = stream.read(&mut buf).await;

                    let body = format!(
                        r#"{{"model":"llama3.2","created_at":"2024-01-01T00:00:00Z","message":{{"role":"assistant","content":"{summary_text}"}},"done":true}}"#
                    );
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body,
                    );
                    let _ = stream.write_all(response.as_bytes()).await;
                }
            }
        }
    });

    (port, shutdown_tx)
}

// ---------------------------------------------------------------------------
// Config and HOME helpers
// ---------------------------------------------------------------------------

fn write_ollama_config(home_dir: &std::path::Path, port: u16) {
    let config_dir = home_dir.join(".config").join("ask");
    std::fs::create_dir_all(&config_dir).expect("config dir should be creatable");
    let config = format!(
        "provider = \"ollama\"\nmodel = \"llama3.2\"\nbase_url = \"http://127.0.0.1:{port}\"\n"
    );
    std::fs::write(config_dir.join("ask.toml"), config).expect("ask.toml write should succeed");
}

fn set_home(path: &std::path::Path) -> Option<String> {
    let prev = std::env::var("HOME").ok();
    std::env::set_var("HOME", path);
    prev
}

fn restore_home(prev: Option<String>) {
    match prev {
        Some(v) => std::env::set_var("HOME", v),
        None => std::env::remove_var("HOME"),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// `context summarize` records a Command entry for the invocation but does not
/// record its output — the transcript contains exactly the seeded entries plus
/// one Command entry for the summarize call and nothing else.
#[tokio::test(flavor = "multi_thread")]
async fn context_summarize_output_not_recorded_in_transcript() {
    let _guard = setup().await;

    let (port, _server) = spawn_mock_ollama("mock summary text").await;
    let tmp = tempdir();
    write_ollama_config(&tmp, port);
    let prev_home = set_home(&tmp);

    run("echo seed").await.expect("run should not error");
    run("context summarize")
        .await
        .expect("run should not error");

    restore_home(prev_home);

    assert_eq!(
        entries(),
        vec![
            ("command", "echo seed".into()),
            ("output", "seed".into()),
            ("command", "context summarize".into()),
        ]
    );
}

/// After `context summarize` runs, a subsequent `context show` also records
/// only its Command entry — neither inspection command leaves an Output entry.
#[tokio::test(flavor = "multi_thread")]
async fn context_summarize_then_show_neither_records_output() {
    let _guard = setup().await;

    let (port, _server) = spawn_mock_ollama("summary for show test").await;
    let tmp = tempdir();
    write_ollama_config(&tmp, port);
    let prev_home = set_home(&tmp);

    run("echo seed").await.expect("run should not error");
    run("context summarize")
        .await
        .expect("run should not error");
    restore_home(prev_home);

    run("context show").await.expect("run should not error");

    assert_eq!(
        entries(),
        vec![
            ("command", "echo seed".into()),
            ("output", "seed".into()),
            ("command", "context summarize".into()),
            ("command", "context show".into()),
        ]
    );
}

/// `context show` does not record its output even when it has non-empty content
/// to print (because the transcript is non-empty at call time).
#[tokio::test]
async fn context_show_output_not_recorded() {
    let _guard = setup().await;

    run("echo marker").await.expect("run should not error");
    run("context show").await.expect("run should not error");

    assert_eq!(
        entries(),
        vec![
            ("command", "echo marker".into()),
            ("output", "marker".into()),
            ("command", "context show".into()),
        ]
    );
}

/// `context summarize` on an empty transcript exits 0 and records only its
/// Command entry — the "(transcript is empty)" message is not recorded as
/// an Output entry.
#[tokio::test]
async fn context_summarize_empty_transcript_records_only_command_entry() {
    let _guard = setup().await;

    // No config needed — the empty-transcript path returns before the provider
    // call.  We still redirect HOME so any accidental config read gives a clear
    // NotConfigured error rather than using the developer's real ask.toml.
    let tmp = tempdir();
    let prev_home = set_home(&tmp);

    run("context summarize")
        .await
        .expect("run should not error");

    restore_home(prev_home);

    assert_eq!(entries(), vec![("command", "context summarize".into())]);
}

/// `context summarize` in interactive mode follows the same recording rules:
/// the invocation is recorded as a Command entry, its output is not.
#[tokio::test(flavor = "multi_thread")]
async fn context_summarize_interactive_output_not_recorded() {
    let _guard = setup().await;

    let (port, _server) = spawn_mock_ollama("interactive summary text").await;
    let tmp = tempdir();
    write_ollama_config(&tmp, port);
    let prev_home = set_home(&tmp);

    let mut shell = Shell::new(default_options())
        .await
        .expect("shell creation should not error");
    run_interactive(
        &mut shell,
        Cursor::new(b"echo hello\ncontext summarize\n"),
        std::io::sink(),
    )
    .await
    .expect("run_interactive should not error");

    restore_home(prev_home);

    assert_eq!(
        entries(),
        vec![
            ("command", "echo hello".into()),
            ("output", "hello".into()),
            ("command", "context summarize".into()),
        ]
    );
}

// ---------------------------------------------------------------------------
// Utility
// ---------------------------------------------------------------------------

fn tempdir() -> TempDir {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("clank-test-summarize-{n}"));
    std::fs::create_dir_all(&path).expect("temp dir should be creatable");
    TempDir(path)
}

struct TempDir(PathBuf);

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

impl std::ops::Deref for TempDir {
    type Target = PathBuf;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
