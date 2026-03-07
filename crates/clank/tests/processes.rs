/// Level 2 tests for the `AskProcess` and `ModelProcess` adapters in
/// `crates/clank/src/processes.rs`.
///
/// These verify the contracts of the thin bridge layer between the shell
/// dispatch and the pure `run_ask` / `run_model` functions.
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use brush_core::openfiles::OpenFile;
use clank_http::{MockHttpClient, MockResponse};
use clank_shell::process::{Process, ProcessContext, ProcessIo};
use clank_shell::{EntryKind, Transcript};
use tokio::sync::Mutex;

use clank::processes::AskProcess;

/// Build an OpenRouter-format success response JSON.
fn openrouter_success(text: &str) -> MockResponse {
    let body = serde_json::json!({
        "id": "gen-test",
        "choices": [{"message": {"role": "assistant", "content": text}}],
        "model": "anthropic/claude-sonnet-4-5",
        "usage": {"prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15}
    });
    MockResponse::json(body.to_string())
}

fn openrouter_config(dir: &std::path::Path) -> std::path::PathBuf {
    let path = dir.join("ask.toml");
    std::fs::write(
        &path,
        "default_model = \"anthropic/claude-sonnet-4-5\"\n\
         [providers.openrouter]\napi_key = \"sk-or-test\"\n",
    )
    .unwrap();
    path
}

/// Serialise tests that set CLANK_CONFIG to prevent parallel env var pollution.
static ENV_LOCK: Mutex<()> = Mutex::const_new(());

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_ctx(argv: Vec<&str>, stdout: std::fs::File, stderr: std::fs::File) -> ProcessContext {
    ProcessContext {
        argv: argv.into_iter().map(str::to_string).collect(),
        env: HashMap::new(),
        io: ProcessIo {
            stdin: OpenFile::Stdin(std::io::stdin()),
            stdout: OpenFile::from(stdout),
            stderr: OpenFile::from(stderr),
        },
        pid: 0,
        cwd: std::path::PathBuf::from("/"),
    }
}

/// Build an Anthropic-format success response JSON.
fn anthropic_success(text: &str) -> MockResponse {
    let body = serde_json::json!({
        "content": [{"type": "text", "text": text}],
        "id": "msg_test",
        "model": "claude-sonnet-4-5",
        "role": "assistant",
        "stop_reason": "end_turn",
        "type": "message",
        "usage": {"input_tokens": 5, "output_tokens": 5}
    });
    MockResponse::json(body.to_string())
}

/// Build an AskProcess with a mock HTTP client and a pre-configured
/// anthropic API key so `run_ask` doesn't fail with "no key configured".
fn ask_process_with_mock(
    transcript: Arc<RwLock<Transcript>>,
    responses: Vec<MockResponse>,
) -> AskProcess {
    // Inject an isolated config via CLANK_CONFIG so run_ask finds an API key.
    // The caller is responsible for cleaning up the env var.
    let http = Arc::new(MockHttpClient::new(responses));
    AskProcess::new(http, transcript)
}

// ---------------------------------------------------------------------------
// F14 — AskProcess behavioural contracts
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_ask_process_appends_ai_response_to_transcript() {
    // Acquire lock to serialise env var access, then drop before any await.
    let _lock = ENV_LOCK.lock().await;
    // The central contract: a successful AI response must be recorded in the
    // transcript as an AiResponse entry so future ask calls have context.
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("ask.toml");
    std::fs::write(
        &config_path,
        "[providers.anthropic]\napi_key = \"sk-test\"\n",
    )
    .unwrap();
    std::env::set_var("CLANK_CONFIG", &config_path);

    let transcript = Arc::new(RwLock::new(Transcript::default()));
    let proc = ask_process_with_mock(
        Arc::clone(&transcript),
        vec![anthropic_success("The answer is 42.")],
    );

    let out = tempfile::NamedTempFile::new().unwrap();
    let err = tempfile::NamedTempFile::new().unwrap();
    let ctx = make_ctx(
        vec!["ask", "what is the answer?"],
        out.reopen().unwrap(),
        err.reopen().unwrap(),
    );

    let result = proc.run(ctx).await;
    std::env::remove_var("CLANK_CONFIG");

    assert_eq!(result.exit_code, 0);

    // The AI response must be recorded in the transcript.
    let t = transcript.read().unwrap();
    let ai_entries: Vec<_> = t
        .entries()
        .iter()
        .filter(|e| e.kind == EntryKind::AiResponse)
        .collect();
    assert!(
        !ai_entries.is_empty(),
        "AI response must be appended to transcript"
    );
    assert!(
        ai_entries
            .iter()
            .any(|e| e.text.contains("The answer is 42")),
        "AI response content missing from transcript: {ai_entries:?}"
    );
}

#[tokio::test]
async fn test_ask_process_does_not_append_on_error() {
    // Acquire lock to serialise env var access, then drop before any await.
    let _lock = ENV_LOCK.lock().await;
    // When ask fails (no API key), the transcript must not receive a spurious
    // AiResponse entry.
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("ask.toml");
    // Config exists but has no API key.
    std::fs::write(&config_path, "# no key\n").unwrap();
    std::env::set_var("CLANK_CONFIG", &config_path);

    let transcript = Arc::new(RwLock::new(Transcript::default()));
    let proc = ask_process_with_mock(Arc::clone(&transcript), vec![]);

    let out = tempfile::NamedTempFile::new().unwrap();
    let err = tempfile::NamedTempFile::new().unwrap();
    let ctx = make_ctx(
        vec!["ask", "hello"],
        out.reopen().unwrap(),
        err.reopen().unwrap(),
    );

    let result = proc.run(ctx).await;
    std::env::remove_var("CLANK_CONFIG");

    assert_ne!(result.exit_code, 0, "must fail without API key");

    let t = transcript.read().unwrap();
    let ai_entries: Vec<_> = t
        .entries()
        .iter()
        .filter(|e| e.kind == EntryKind::AiResponse)
        .collect();
    assert!(
        ai_entries.is_empty(),
        "no AiResponse must be recorded on error: {ai_entries:?}"
    );
}

#[tokio::test]
async fn test_ask_process_routes_stdout_to_io_handle() {
    // Acquire lock to serialise env var access, then drop before any await.
    let _lock = ENV_LOCK.lock().await;
    // AI response text must go to the process's stdout handle.
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("ask.toml");
    std::fs::write(
        &config_path,
        "[providers.anthropic]\napi_key = \"sk-test\"\n",
    )
    .unwrap();
    std::env::set_var("CLANK_CONFIG", &config_path);

    let transcript = Arc::new(RwLock::new(Transcript::default()));
    let proc = ask_process_with_mock(
        Arc::clone(&transcript),
        vec![anthropic_success("Hello from the model.")],
    );

    let out = tempfile::NamedTempFile::new().unwrap();
    let err = tempfile::NamedTempFile::new().unwrap();
    let ctx = make_ctx(
        vec!["ask", "say hello"],
        out.reopen().unwrap(),
        err.reopen().unwrap(),
    );

    proc.run(ctx).await;
    std::env::remove_var("CLANK_CONFIG");

    let stdout = std::fs::read_to_string(out.path()).unwrap();
    assert!(
        stdout.contains("Hello from the model."),
        "AI response must appear on stdout: {stdout}"
    );
    let stderr = std::fs::read_to_string(err.path()).unwrap();
    assert!(
        stderr.is_empty(),
        "stderr must be empty on success: {stderr}"
    );
}

#[tokio::test]
async fn test_ask_process_routes_error_to_stderr() {
    // Acquire lock to serialise env var access, then drop before any await.
    let _lock = ENV_LOCK.lock().await;
    // Error messages (no API key, bad flags) must go to stderr, not stdout.
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("ask.toml");
    std::fs::write(&config_path, "# no key\n").unwrap();
    std::env::set_var("CLANK_CONFIG", &config_path);

    let transcript = Arc::new(RwLock::new(Transcript::default()));
    let proc = ask_process_with_mock(Arc::clone(&transcript), vec![]);

    let out = tempfile::NamedTempFile::new().unwrap();
    let err = tempfile::NamedTempFile::new().unwrap();
    let ctx = make_ctx(
        vec!["ask", "hello"],
        out.reopen().unwrap(),
        err.reopen().unwrap(),
    );

    proc.run(ctx).await;
    std::env::remove_var("CLANK_CONFIG");

    let stdout = std::fs::read_to_string(out.path()).unwrap();
    let stderr = std::fs::read_to_string(err.path()).unwrap();
    assert!(stdout.is_empty(), "stdout must be empty on error: {stdout}");
    assert!(!stderr.is_empty(), "error message must appear on stderr");
}

#[tokio::test]
async fn test_ask_process_sends_transcript_in_request() {
    // Acquire lock to serialise env var access, then drop before any await.
    let _lock = ENV_LOCK.lock().await;

    // End-to-end contract: prior transcript content must appear in the
    // outgoing HTTP request body sent to the model. This test catches the
    // class of bug where the system prompt (which embeds the transcript) is
    // silently dropped by the provider's wire serialisation.
    //
    // We use the OpenRouter provider because:
    //   (a) it was the specific provider affected by the system-prompt bug,
    //   (b) it uses the OpenAI wire format where the system prompt must appear
    //       as messages[0] with role "system" — not a top-level field.
    let dir = tempfile::tempdir().unwrap();
    let config_path = openrouter_config(dir.path());
    std::env::set_var("CLANK_CONFIG", &config_path);

    let transcript = Arc::new(RwLock::new(Transcript::default()));

    // Pre-populate the transcript with a command and its output, exactly as
    // `ClankShell::run_line` would after executing a real command.
    {
        let mut t = transcript.write().unwrap();
        t.append(EntryKind::Command, "ls -la", false);
        t.append(EntryKind::Output, "total 8\ndrwxr-xr-x sentinel_dir", false);
    }

    let mock = Arc::new(MockHttpClient::new(vec![openrouter_success(
        "I can see the ls output.",
    )]));
    let proc = AskProcess::new(
        Arc::clone(&mock) as Arc<dyn clank_http::HttpClient>,
        Arc::clone(&transcript),
    );

    let out = tempfile::NamedTempFile::new().unwrap();
    let err = tempfile::NamedTempFile::new().unwrap();
    let ctx = make_ctx(
        vec!["ask", "what does that output tell me?"],
        out.reopen().unwrap(),
        err.reopen().unwrap(),
    );

    let result = proc.run(ctx).await;
    std::env::remove_var("CLANK_CONFIG");

    assert_eq!(result.exit_code, 0, "ask must succeed");

    // Inspect the outgoing HTTP request body.
    let reqs = mock.requests.lock().await;
    assert_eq!(reqs.len(), 1, "exactly one HTTP request must be made");
    let body: serde_json::Value = serde_json::from_slice(reqs[0].body.as_deref().unwrap()).unwrap();

    // The transcript is embedded in the system prompt, which must appear as
    // the first message with role "system" in the OpenAI wire format.
    // https://platform.openai.com/docs/api-reference/chat/create
    let messages = body["messages"]
        .as_array()
        .expect("messages must be an array");
    assert_eq!(
        messages[0]["role"], "system",
        "first message must be the system prompt"
    );
    let system_content = messages[0]["content"].as_str().unwrap();
    assert!(
        system_content.contains("sentinel_dir"),
        "transcript output must appear in the system prompt; got: {system_content}"
    );
    assert!(
        system_content.contains("ls -la"),
        "transcript command must appear in the system prompt; got: {system_content}"
    );
}
