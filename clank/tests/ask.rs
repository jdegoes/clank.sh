//! Integration tests for the `ask` command.
//!
//! These tests live in the integration test suite (separate process from clank-config
//! unit tests) to avoid HOME env var races.

use async_trait::async_trait;
use clank_http::{HttpClient, HttpError, HttpResponse, RequestHeader};
use std::sync::{Arc, Mutex};



/// A stub HttpClient that returns a canned Anthropic-format response.
struct StubHttpClient {
    response_text: String,
}

#[async_trait]
impl HttpClient for StubHttpClient {
    async fn post_json(
        &self,
        _url: &str,
        _headers: &[RequestHeader],
        _body: &str,
    ) -> Result<HttpResponse, HttpError> {
        Ok(HttpResponse {
            status: 200,
            body: format!(
                r#"{{"content":[{{"type":"text","text":"{}"}}],"stop_reason":"end_turn"}}"#,
                self.response_text
            ),
        })
    }
}

/// An HttpClient that captures the request body for inspection.
struct CapturingHttpClient {
    captured_body: Arc<Mutex<String>>,
}

#[async_trait]
impl HttpClient for CapturingHttpClient {
    async fn post_json(
        &self,
        _url: &str,
        _headers: &[RequestHeader],
        body: &str,
    ) -> Result<HttpResponse, HttpError> {
        *self.captured_body.lock().unwrap() = body.to_string();
        Ok(HttpResponse {
            status: 200,
            body: r#"{"content":[{"type":"text","text":"ok"}],"stop_reason":"end_turn"}"#
                .to_string(),
        })
    }
}

/// Set up a temp HOME with a configured model and API key.
fn setup_config(tmp: &tempfile::TempDir) {
    unsafe {
        std::env::set_var("HOME", tmp.path());
    }
    let mut config = clank_config::AskConfig::default();
    config.add_provider("anthropic".to_string(), "sk-test".to_string());
    config.set_default_model("anthropic/claude-sonnet-4-5".to_string());
    clank_config::save_config(&config).unwrap();
}

#[tokio::test]
async fn run_ask_returns_response_and_records_in_transcript() {
    let tmp = tempfile::tempdir().unwrap();
    setup_config(&tmp);

    let http: Arc<dyn HttpClient> = Arc::new(StubHttpClient {
        response_text: "The answer is 42.".to_string(),
    });

    let mut shell = clank::build_shell().await;
    let result = shell.run_ask("ask what is the answer?", &http).await;

    assert!(result.is_ok(), "run_ask should succeed: {result:?}");
    assert_eq!(result.unwrap(), "The answer is 42.");

    // AI response must be in the transcript.
    let t = shell.transcript_as_string();
    assert!(t.contains("The answer is 42."), "transcript must contain AI response");
}

#[tokio::test]
async fn run_ask_fresh_excludes_transcript_from_request() {
    let tmp = tempfile::tempdir().unwrap();
    setup_config(&tmp);

    let captured = Arc::new(Mutex::new(String::new()));
    let http: Arc<dyn HttpClient> = Arc::new(CapturingHttpClient {
        captured_body: captured.clone(),
    });

    let mut shell = clank::build_shell().await;
    // Run a command that adds to the transcript.
    shell.run_command("echo prior_output").await;
    // ask --fresh must NOT include that transcript context.
    shell
        .run_ask("ask --fresh just the prompt", &http)
        .await
        .unwrap();

    let body = captured.lock().unwrap().clone();
    assert!(
        !body.contains("prior_output"),
        "--fresh ask must not include transcript in request body"
    );
    assert!(body.contains("just the prompt"));
}

#[tokio::test]
async fn run_ask_with_transcript_includes_prior_commands() {
    let tmp = tempfile::tempdir().unwrap();
    setup_config(&tmp);

    let captured = Arc::new(Mutex::new(String::new()));
    let http: Arc<dyn HttpClient> = Arc::new(CapturingHttpClient {
        captured_body: captured.clone(),
    });

    let mut shell = clank::build_shell().await;
    shell.run_command("echo hello_world").await;
    shell.run_ask("ask what did I run?", &http).await.unwrap();

    let body = captured.lock().unwrap().clone();
    assert!(
        body.contains("hello_world"),
        "default ask must include transcript context in request body"
    );
}
