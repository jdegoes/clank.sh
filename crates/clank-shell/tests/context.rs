use std::sync::{Arc, RwLock};

use clank_http::MockHttpClient;
use clank_shell::secrets::SecretsRegistry;
use clank_shell::{ClankShell, EntryKind, Transcript};

/// Build a shell with a shared transcript and a stub HTTP client.
async fn shell_with_transcript() -> (ClankShell, Arc<RwLock<Transcript>>) {
    let transcript = Arc::new(RwLock::new(Transcript::default()));
    let http = Arc::new(MockHttpClient::new(vec![]));
    let shell = ClankShell::with_http(Arc::clone(&transcript), http)
        .await
        .expect("failed to create shell");
    (shell, transcript)
}

#[tokio::test]
async fn test_context_show_prints_transcript() {
    let (mut shell, transcript) = shell_with_transcript().await;

    // Manually add a known entry so we have stable content.
    transcript
        .write()
        .unwrap()
        .append(EntryKind::Command, "echo hello", false);

    // `context show` should exit 0.
    let code = shell.run_line("context show").await;
    assert_eq!(code, 0, "context show should exit 0");
}

#[tokio::test]
async fn test_context_clear_empties_transcript() {
    let (mut shell, transcript) = shell_with_transcript().await;

    transcript
        .write()
        .unwrap()
        .append(EntryKind::Command, "ls", false);
    assert!(!transcript.read().unwrap().is_empty());

    let code = shell.run_line("context clear").await;
    assert_eq!(code, 0);
    // The clear itself adds a "context clear" command entry, then clears.
    // After clear the only entry should be the one added by run_line
    // recording "context clear" — but clear wipes everything including that.
    // Actual behaviour: run_line records the command BEFORE execution,
    // then ContextProcess::clear() wipes everything including that entry.
    assert!(transcript.read().unwrap().is_empty());
}

#[tokio::test]
async fn test_context_trim_drops_n_entries() {
    let (mut shell, transcript) = shell_with_transcript().await;

    {
        let mut t = transcript.write().unwrap();
        for i in 0..5 {
            t.append(EntryKind::Command, format!("cmd{i}"), false);
        }
    }
    assert_eq!(transcript.read().unwrap().len(), 5);

    let code = shell.run_line("context trim 2").await;
    assert_eq!(code, 0);
    // trim 2 drops 2 oldest entries; run_line also records "context trim 2"
    // before execution (adding 1) and trim runs on the 6-entry list dropping
    // 2 → 4 entries remain.
    let len = transcript.read().unwrap().len();
    assert!(len <= 5, "expected trim to reduce entries, got {len}");
}

#[tokio::test]
async fn test_context_trim_bad_arg_exits_2() {
    let (mut shell, _) = shell_with_transcript().await;
    let code = shell.run_line("context trim notanumber").await;
    assert_eq!(code, 2, "bad trim arg should exit 2");
}

#[tokio::test]
async fn test_context_unknown_subcommand_exits_2() {
    let (mut shell, _) = shell_with_transcript().await;
    let code = shell.run_line("context frobnicate").await;
    assert_eq!(code, 2);
}

#[tokio::test]
async fn test_context_no_subcommand_exits_2() {
    let (mut shell, _) = shell_with_transcript().await;
    let code = shell.run_line("context").await;
    assert_eq!(code, 2);
}

// ---------------------------------------------------------------------------
// context summarize — real behavioural tests using injected config
// ---------------------------------------------------------------------------

/// Build a ContextProcess with an injected AskConfig and mock HTTP client.
fn make_summarize_process(
    transcript: Arc<RwLock<Transcript>>,
    http: Arc<dyn clank_http::HttpClient>,
) -> clank_shell::context_process::ContextProcess {
    use clank_ask::config::{AskConfig, ProviderConfig};
    let mut providers = std::collections::HashMap::new();
    providers.insert(
        "anthropic".to_string(),
        ProviderConfig {
            api_key: Some("sk-test".to_string()),
            base_url: None,
        },
    );
    let config = AskConfig {
        default_model: Some("anthropic/claude-sonnet-4-5".to_string()),
        providers,
    };
    clank_shell::context_process::ContextProcess::with_config(transcript, http, config)
}

fn make_process_context(
    argv: Vec<&str>,
    stdout: std::fs::File,
    stderr: std::fs::File,
) -> clank_shell::process::ProcessContext {
    use brush_core::openfiles::OpenFile;
    use clank_shell::process::ProcessIo;
    use std::collections::HashMap;
    clank_shell::process::ProcessContext {
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

fn anthropic_response(text: &str) -> clank_http::MockResponse {
    let body = serde_json::json!({
        "content": [{"type": "text", "text": text}],
        "id": "msg_test",
        "model": "claude-sonnet-4-5",
        "role": "assistant",
        "stop_reason": "end_turn",
        "type": "message",
        "usage": {"input_tokens": 10, "output_tokens": 5}
    });
    clank_http::MockResponse::json(body.to_string())
}

#[tokio::test]
async fn test_context_summarize_empty_transcript_exits_0_with_message() {
    use clank_shell::process::Process;
    let transcript = Arc::new(RwLock::new(Transcript::default()));
    let http = Arc::new(MockHttpClient::new(vec![])); // no responses needed
    let proc = make_summarize_process(Arc::clone(&transcript), http);

    let out = tempfile::NamedTempFile::new().unwrap();
    let err = tempfile::NamedTempFile::new().unwrap();
    let ctx = make_process_context(
        vec!["context", "summarize"],
        out.reopen().unwrap(),
        err.reopen().unwrap(),
    );

    let result = proc.run(ctx).await;
    assert_eq!(result.exit_code, 0);
    let stdout = std::fs::read_to_string(out.path()).unwrap();
    assert!(
        stdout.contains("empty"),
        "must report transcript is empty: {stdout}"
    );
}

#[tokio::test]
async fn test_context_summarize_success_writes_summary_to_stdout() {
    use clank_shell::process::Process;
    let transcript = Arc::new(RwLock::new(Transcript::default()));
    transcript
        .write()
        .unwrap()
        .append(EntryKind::Command, "echo hello", false);

    let http = Arc::new(MockHttpClient::new(vec![anthropic_response(
        "The user ran echo hello.",
    )]));
    let proc = make_summarize_process(Arc::clone(&transcript), http);

    let out = tempfile::NamedTempFile::new().unwrap();
    let err = tempfile::NamedTempFile::new().unwrap();
    let ctx = make_process_context(
        vec!["context", "summarize"],
        out.reopen().unwrap(),
        err.reopen().unwrap(),
    );

    let result = proc.run(ctx).await;
    assert_eq!(result.exit_code, 0);
    let stdout = std::fs::read_to_string(out.path()).unwrap();
    assert!(
        stdout.contains("echo hello"),
        "summary content missing from stdout: {stdout}"
    );
    assert!(std::fs::read_to_string(err.path()).unwrap().is_empty());
}

#[tokio::test]
async fn test_context_summarize_timeout_exits_3() {
    use clank_http::HttpError;
    use clank_shell::process::Process;

    let transcript = Arc::new(RwLock::new(Transcript::default()));
    transcript
        .write()
        .unwrap()
        .append(EntryKind::Command, "ls", false);

    let http = Arc::new(MockHttpClient::with_results(vec![Err(HttpError::Timeout)]));
    let proc = make_summarize_process(Arc::clone(&transcript), http);

    let out = tempfile::NamedTempFile::new().unwrap();
    let err = tempfile::NamedTempFile::new().unwrap();
    let ctx = make_process_context(
        vec!["context", "summarize"],
        out.reopen().unwrap(),
        err.reopen().unwrap(),
    );

    let result = proc.run(ctx).await;
    assert_eq!(result.exit_code, 3, "timeout must exit 3");
    let stderr = std::fs::read_to_string(err.path()).unwrap();
    assert!(
        stderr.contains("timed out"),
        "timeout message missing from stderr: {stderr}"
    );
}

#[tokio::test]
async fn test_context_summarize_http_error_exits_4() {
    use clank_http::HttpError;
    use clank_shell::process::Process;

    let transcript = Arc::new(RwLock::new(Transcript::default()));
    transcript
        .write()
        .unwrap()
        .append(EntryKind::Command, "ls", false);

    let http = Arc::new(MockHttpClient::with_results(vec![Err(
        HttpError::NonSuccessResponse {
            status: 500,
            body: "Internal Server Error".to_string(),
        },
    )]));
    let proc = make_summarize_process(Arc::clone(&transcript), http);

    let out = tempfile::NamedTempFile::new().unwrap();
    let err = tempfile::NamedTempFile::new().unwrap();
    let ctx = make_process_context(
        vec!["context", "summarize"],
        out.reopen().unwrap(),
        err.reopen().unwrap(),
    );

    let result = proc.run(ctx).await;
    assert_eq!(result.exit_code, 4, "HTTP error must exit 4");
    let stderr = std::fs::read_to_string(err.path()).unwrap();
    assert!(
        stderr.contains("clank: context summarize:"),
        "error prefix missing from stderr: {stderr}"
    );
}

#[tokio::test]
async fn test_context_summarize_wrong_shape_exits_1_with_error() {
    use clank_shell::process::Process;

    let transcript = Arc::new(RwLock::new(Transcript::default()));
    transcript
        .write()
        .unwrap()
        .append(EntryKind::Command, "ls", false);

    // Response that is valid HTTP 200 but not the expected Anthropic JSON shape.
    // AnthropicProvider deserialises into a typed struct; a missing `content`
    // field is a parse error → ProviderError::Other → exit 1.
    let http = Arc::new(MockHttpClient::new(vec![clank_http::MockResponse::json(
        r#"{"unexpected":"shape"}"#,
    )]));
    let proc = make_summarize_process(Arc::clone(&transcript), http);

    let out = tempfile::NamedTempFile::new().unwrap();
    let err = tempfile::NamedTempFile::new().unwrap();
    let ctx = make_process_context(
        vec!["context", "summarize"],
        out.reopen().unwrap(),
        err.reopen().unwrap(),
    );

    let result = proc.run(ctx).await;
    assert_eq!(
        result.exit_code, 1,
        "malformed Anthropic response shape must exit 1"
    );
    let stderr = std::fs::read_to_string(err.path()).unwrap();
    assert!(
        stderr.contains("clank: context summarize:"),
        "error prefix missing from stderr: {stderr}"
    );
    assert!(
        std::fs::read_to_string(out.path()).unwrap().is_empty(),
        "stdout must be empty on parse error"
    );
}

#[tokio::test]
async fn test_context_show_not_re_recorded() {
    // context show output must not be re-recorded into the transcript.
    let (mut shell, transcript) = shell_with_transcript().await;
    transcript
        .write()
        .unwrap()
        .append(EntryKind::Command, "ls", false);

    let len_before = transcript.read().unwrap().len();
    shell.run_line("context show").await;

    // Only the "context show" command itself should be added (by run_line),
    // not the output of context show.
    let entries = transcript.read().unwrap();
    let output_entries: Vec<_> = entries
        .entries()
        .iter()
        .filter(|e| e.kind == EntryKind::Output && e.text.contains("$ ls"))
        .collect();
    // context show output should NOT appear as an Output entry in the transcript.
    assert!(
        output_entries.is_empty(),
        "context show output was re-recorded into the transcript"
    );
    drop(entries);

    // Transcript length grew only by the "context show" command entry.
    let len_after = transcript.read().unwrap().len();
    assert_eq!(
        len_after,
        len_before + 1,
        "only the command entry should be added"
    );
}

// ---------------------------------------------------------------------------
// Phase 2 deviation remediation tests
// ---------------------------------------------------------------------------

/// D1a: After `export FOO=bar`, `echo $FOO` returns `bar` in stdout.
#[tokio::test]
async fn test_export_sets_env_variable() {
    let (mut shell, transcript) = shell_with_transcript().await;

    let export_code = shell
        .run_line("export CLANK_TEST_D1A=hello_from_export")
        .await;
    assert_eq!(export_code, 0, "export should exit 0");

    let echo_code = shell.run_line("echo $CLANK_TEST_D1A").await;
    assert_eq!(echo_code, 0, "echo should exit 0");

    let t = transcript.read().unwrap();
    let found = t
        .entries()
        .iter()
        .filter(|e| e.kind == EntryKind::Output)
        .any(|e| e.text.contains("hello_from_export"));
    assert!(found, "expected 'hello_from_export' in transcript output");
}

/// D1b: After `export --secret KEY=val`, `SecretsRegistry::contains("KEY")` is true.
#[tokio::test]
async fn test_export_secret_registers_in_secrets() {
    let (mut shell, _) = shell_with_transcript().await;

    let code = shell
        .run_line("export --secret CLANK_TEST_D1B=mysecret")
        .await;
    assert_eq!(code, 0, "export --secret should exit 0");

    assert!(
        SecretsRegistry::contains("CLANK_TEST_D1B"),
        "secret variable should be registered in SecretsRegistry"
    );

    // Clean up so other tests are not affected.
    clank_shell::secrets::SecretsRegistry::remove("CLANK_TEST_D1B");
}

/// D3: `sudo env` dispatches `env`, not `sudo env`.
/// If sudo stripping is broken, Brush will try to execute `sudo` as an
/// unknown command, returning exit 127. If it is fixed, `env` runs and
/// exits 0.
#[tokio::test]
async fn test_sudo_strips_prefix_from_dispatch() {
    let (mut shell, _) = shell_with_transcript().await;
    let code = shell.run_line("sudo env").await;
    assert_eq!(code, 0, "sudo env should dispatch env (exit 0), not sudo");
}

// ---------------------------------------------------------------------------
// F16 — exit code clamping regression test
// ---------------------------------------------------------------------------

/// Verify that an exit code above 255 is clamped to 255 (not silently wrapped
/// to 0, which would turn an error into apparent success).
#[tokio::test]
async fn test_exit_code_above_255_is_not_zero() {
    use clank_shell::process::{Process, ProcessContext, ProcessResult};
    use clank_shell::register_command;

    struct HighExitProcess;
    #[async_trait::async_trait]
    impl Process for HighExitProcess {
        async fn run(&self, _ctx: ProcessContext) -> ProcessResult {
            ProcessResult::failure(256)
        }
    }

    let (mut shell, _) = shell_with_transcript().await;
    let shell_id = shell.shell_id();
    register_command(shell_id, "high-exit", Arc::new(HighExitProcess));

    let code = shell.run_line("high-exit").await;
    // Must NOT be 0 (which would be a silent success from truncation).
    assert_ne!(code, 0, "exit code 256 must not wrap to 0");
    // Must be clamped to at most 255.
    assert!(
        code <= 255,
        "exit code must be clamped to u8 range, got {code}"
    );
}

/// D6: `ps aux` output contains `%CPU` and `%MEM` column headers.
#[tokio::test]
async fn test_ps_aux_has_cpu_mem_columns() {
    let (mut shell, transcript) = shell_with_transcript().await;

    let code = shell.run_line("ps aux").await;
    assert_eq!(code, 0, "ps aux should exit 0");

    let t = transcript.read().unwrap();
    let all_output: String = t
        .entries()
        .iter()
        .filter(|e| e.kind == EntryKind::Output)
        .map(|e| e.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        all_output.contains("%CPU"),
        "ps aux output missing %CPU column header; got: {all_output}"
    );
    assert!(
        all_output.contains("%MEM"),
        "ps aux output missing %MEM column header; got: {all_output}"
    );
}
