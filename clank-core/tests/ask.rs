//! Integration tests for the `ask` command transcript-recording semantics.
//!
//! Verifies that:
//! - `ask` records an `AiResponse` entry in the transcript after a successful call.
//! - `ask` response text is NOT recorded as an `Output` entry (only as `AiResponse`).
//! - `ask --fresh` omits transcript context from the provider request but still
//!   records the response.
//! - The Command entry for `ask` is always recorded regardless of outcome.
//!
//! Uses the same in-process mock Ollama server pattern as `summarize.rs`.

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
// Transcript helper
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
// Mock Ollama server — captures last request body for inspection
// ---------------------------------------------------------------------------

/// Spawn a mock Ollama server that returns `response_text`.
///
/// Returns `(port, last_request_tx, shutdown_tx)`.
/// `last_request_tx` receives the raw body of each request so tests can
/// assert what was sent to the provider.
async fn spawn_mock_ollama(
    response_text: &'static str,
) -> (
    u16,
    tokio::sync::mpsc::Receiver<String>,
    tokio::sync::oneshot::Sender<()>,
) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("mock server should bind");
    let port = listener
        .local_addr()
        .expect("mock server should have a local addr")
        .port();

    let (shutdown_tx, mut shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let (req_tx, req_rx) = tokio::sync::mpsc::channel::<String>(8);

    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = &mut shutdown_rx => break,
                accepted = listener.accept() => {
                    let (mut stream, _) = accepted.expect("accept should not fail");
                    let req_tx = req_tx.clone();

                    let mut buf = vec![0u8; 16384];
                    let n = stream.read(&mut buf).await.unwrap_or(0);
                    let raw = String::from_utf8_lossy(&buf[..n]).to_string();

                    // Extract the JSON body (everything after the blank line).
                    let body = raw
                        .split_once("\r\n\r\n")
                        .map(|(_, b)| b.to_owned())
                        .unwrap_or_default();
                    let _ = req_tx.try_send(body);

                    let resp_body = format!(
                        r#"{{"model":"llama3.2","created_at":"2024-01-01T00:00:00Z","message":{{"role":"assistant","content":"{response_text}"}},"done":true}}"#
                    );
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        resp_body.len(),
                        resp_body,
                    );
                    let _ = stream.write_all(response.as_bytes()).await;
                }
            }
        }
    });

    (port, req_rx, shutdown_tx)
}

// ---------------------------------------------------------------------------
// Config / HOME helpers (same pattern as summarize.rs)
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

/// A successful `ask` call records `Command("ask ...")` and
/// `AiResponse("<response>")` — no `Output` entry for the response text.
#[tokio::test(flavor = "multi_thread")]
async fn ask_records_ai_response_not_output() {
    let _guard = setup().await;

    let (port, _req_rx, _server) = spawn_mock_ollama("mock ai response").await;
    let tmp = tempdir();
    write_ollama_config(&tmp, port);
    let prev_home = set_home(&tmp);

    run("ask hello").await.expect("run should not error");

    restore_home(prev_home);

    assert_eq!(
        entries(),
        vec![
            ("command", "ask hello".into()),
            ("ai_response", "mock ai response".into()),
        ]
    );
}

/// With prior context in the transcript, `ask` includes it in the request
/// body sent to the provider, and still records only Command + AiResponse.
#[tokio::test(flavor = "multi_thread")]
async fn ask_includes_transcript_context_and_records_ai_response() {
    let _guard = setup().await;

    let (port, mut req_rx, _server) = spawn_mock_ollama("response with context").await;
    let tmp = tempdir();
    write_ollama_config(&tmp, port);
    let prev_home = set_home(&tmp);

    run("echo seed").await.expect("run should not error");
    run("ask what is this").await.expect("run should not error");

    restore_home(prev_home);

    // Transcript: seed command/output, then ask command + ai_response.
    assert_eq!(
        entries(),
        vec![
            ("command", "echo seed".into()),
            ("output", "seed".into()),
            ("command", "ask what is this".into()),
            ("ai_response", "response with context".into()),
        ]
    );

    // The request body sent to the mock must contain the transcript context.
    let req_body = req_rx
        .try_recv()
        .expect("mock should have received a request");
    assert!(
        req_body.contains("echo seed"),
        "request body should contain transcript context; got: {req_body}"
    );
}

/// `ask --fresh` sends only the prompt to the provider — no transcript context —
/// but still records an AiResponse entry.
#[tokio::test(flavor = "multi_thread")]
async fn ask_fresh_omits_transcript_from_request_but_records_response() {
    let _guard = setup().await;

    let (port, mut req_rx, _server) = spawn_mock_ollama("fresh response").await;
    let tmp = tempdir();
    write_ollama_config(&tmp, port);
    let prev_home = set_home(&tmp);

    run("echo seed").await.expect("run should not error");
    run("ask --fresh hello")
        .await
        .expect("run should not error");

    restore_home(prev_home);

    // AiResponse is still recorded despite --fresh.
    assert_eq!(
        entries(),
        vec![
            ("command", "echo seed".into()),
            ("output", "seed".into()),
            ("command", "ask --fresh hello".into()),
            ("ai_response", "fresh response".into()),
        ]
    );

    // The request body must NOT contain transcript context.
    let req_body = req_rx
        .try_recv()
        .expect("mock should have received a request");
    assert!(
        !req_body.contains("echo seed"),
        "request body must not contain transcript when --fresh is set; got: {req_body}"
    );
}

/// `ask` in interactive mode records Command + AiResponse, no Output entry.
#[tokio::test(flavor = "multi_thread")]
async fn ask_interactive_records_ai_response_not_output() {
    let _guard = setup().await;

    let (port, _req_rx, _server) = spawn_mock_ollama("interactive response").await;
    let tmp = tempdir();
    write_ollama_config(&tmp, port);
    let prev_home = set_home(&tmp);

    let mut shell = Shell::new(default_options())
        .await
        .expect("shell creation should not error");
    run_interactive(&mut shell, Cursor::new(b"ask hello\n"), std::io::sink())
        .await
        .expect("run_interactive should not error");

    restore_home(prev_home);

    assert_eq!(
        entries(),
        vec![
            ("command", "ask hello".into()),
            ("ai_response", "interactive response".into()),
        ]
    );
}

/// `ask` with no prompt exits 2 and records only the Command entry.
#[tokio::test(flavor = "multi_thread")]
async fn ask_no_prompt_exits_2_records_command() {
    let _guard = setup().await;

    let exit = run("ask").await.expect("run should not error");
    assert_eq!(exit, 2);
    assert_eq!(entries(), vec![("command", "ask".into())]);
}

/// `ask` exits 0 on success against the mock provider.
#[tokio::test(flavor = "multi_thread")]
async fn ask_exits_zero_on_success() {
    let _guard = setup().await;

    let (port, _req_rx, _server) = spawn_mock_ollama("ok").await;
    let tmp = tempdir();
    write_ollama_config(&tmp, port);
    let prev_home = set_home(&tmp);

    let exit = run("ask hello").await.expect("run should not error");

    restore_home(prev_home);
    assert_eq!(exit, 0);
}

// ---------------------------------------------------------------------------
// Utility
// ---------------------------------------------------------------------------

fn tempdir() -> TempDir {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("clank-test-ask-{n}"));
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
