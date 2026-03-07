use std::sync::{Arc, RwLock};

/// The result of a `ask` invocation.
///
/// `stderr` is `Vec<u8>` rather than `String` to preserve binary-safe error
/// output (provider responses may contain non-UTF-8 bytes).
#[derive(Debug)]
pub struct AskOutput {
    pub stdout: String,
    pub stderr: Vec<u8>,
    pub exit_code: i32,
}

use crate::config::AskConfig;
use crate::provider::anthropic::AnthropicProvider;
use crate::provider::ollama::OllamaProvider;
use crate::provider::openai_compat::OpenAiCompatProvider;
use crate::provider::openrouter::OpenRouterProvider;
use crate::provider::{CompletionRequest, Message, ModelProvider, ProviderError, Role};

// clank-shell types are injected via trait objects to avoid a circular
// dependency. The AskProcess receives a transcript snapshot as a
// `Arc<dyn TranscriptReader>` and an `Arc<dyn HttpClient>`.

use clank_http::HttpClient;

/// Provides a read-only view of the session transcript.
/// Implemented by ClankShell; injected into AskProcess.
pub trait TranscriptSnapshot: Send + Sync {
    /// Return the transcript window formatted for the model.
    fn format_for_model(&self) -> String;
    /// Return true if the transcript is empty.
    fn is_empty(&self) -> bool;
}

/// A snapshot of the transcript stored as a plain string.
/// Built by ClankShell before dispatching `ask`.
pub struct StringTranscriptSnapshot(pub String);

impl TranscriptSnapshot for StringTranscriptSnapshot {
    fn format_for_model(&self) -> String {
        self.0.clone()
    }
    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// The `ask` subprocess — invokes a model with the transcript as context.
pub struct AskProcess {
    pub http: Arc<dyn HttpClient>,
    pub transcript: Arc<RwLock<dyn TranscriptSnapshot>>,
    /// Current working directory, for system prompt assembly.
    pub cwd: Option<String>,
}

// ---------------------------------------------------------------------------
// Typed error enums (AGENTS.md: error types are typed enums, never stringly typed)
// ---------------------------------------------------------------------------

/// Errors that can occur when parsing `ask` command-line flags.
#[derive(Debug, thiserror::Error)]
pub enum AskFlagError {
    #[error("unknown flag: {0}")]
    UnknownFlag(String),
    #[error("--model requires an argument")]
    ModelMissingArgument,
    #[error("prompt is required")]
    MissingPrompt,
}

/// Errors that can occur when selecting a model provider.
#[derive(Debug, thiserror::Error)]
pub enum ProviderSelectError {
    #[error(
        "no base_url configured for provider 'openai-compat'\n\
         Run: model add openai-compat --url http://localhost:8080\n"
    )]
    MissingBaseUrl,
    #[error(
        "no API key configured for provider '{provider_name}' or 'openrouter'\n\
         To use Anthropic directly:  add [providers.anthropic] api_key = \"...\" to ~/.config/ask/ask.toml\n\
         To use OpenRouter:          add [providers.openrouter] api_key = \"...\" to ~/.config/ask/ask.toml\n"
    )]
    MissingApiKey { provider_name: String },
}

/// Parsed `ask` invocation flags.
#[derive(Debug, Default)]
struct AskFlags {
    prompt: String,
    model: Option<String>,
    json: bool,
    fresh: bool,
    // --inherit is the default; --fresh/--no-transcript override it
}

impl AskFlags {
    fn parse(argv: &[String]) -> Result<Self, AskFlagError> {
        let mut flags = AskFlags::default();
        let mut i = 1usize; // skip argv[0] = "ask"
        while i < argv.len() {
            match argv[i].as_str() {
                "--model" => {
                    i += 1;
                    flags.model = argv.get(i).cloned();
                    if flags.model.is_none() {
                        return Err(AskFlagError::ModelMissingArgument);
                    }
                }
                "--json" => flags.json = true,
                "--fresh" | "--no-transcript" => flags.fresh = true,
                "--inherit" => flags.fresh = false,
                arg if !arg.starts_with('-') => {
                    if flags.prompt.is_empty() {
                        flags.prompt = arg.to_string();
                    } else {
                        flags.prompt.push(' ');
                        flags.prompt.push_str(arg);
                    }
                }
                unknown => {
                    return Err(AskFlagError::UnknownFlag(unknown.to_string()));
                }
            }
            i += 1;
        }
        Ok(flags)
    }
}

// Process is defined in clank-shell. To avoid a circular dependency we
// re-implement the minimal trait here as a local alias. In Phase 2 we will
// extract Process into its own crate. For now clank-ask defines its own
// AskProcess which is registered in the shell via register_command.
//
// clank-shell calls register_command with an Arc<dyn Process>, where Process
// is clank_shell::process::Process. We cannot implement that trait here
// without pulling in clank-shell. Solution: keep AskProcess as a plain struct
// and have clank-shell wrap it in a thin adapter. See shell.rs.

/// Run the ask logic, returning (stdout, stderr, exit_code).
///
/// `config_override` allows tests to inject a config without a real config
/// file. Pass `None` to load from `~/.config/ask/ask.toml`.
pub async fn run_ask(
    argv: &[String],
    piped_stdin: Vec<u8>,
    http: Arc<dyn HttpClient>,
    transcript_text: String,
    cwd: Option<&str>,
    config_override: Option<AskConfig>,
) -> AskOutput {
    let flags = match AskFlags::parse(argv) {
        Ok(f) => f,
        Err(e) => {
            let msg = format!("clank: ask: {e}\nusage: ask [--model MODEL] [--json] [--fresh] [--inherit] PROMPT\n");
            return AskOutput {
                stdout: String::new(),
                stderr: msg.into_bytes(),
                exit_code: 2,
            };
        }
    };

    if flags.prompt.is_empty() {
        let msg = "clank: ask: prompt is required\nusage: ask [--model MODEL] [--json] [--fresh] [--inherit] PROMPT\n";
        return AskOutput {
            stdout: String::new(),
            stderr: msg.as_bytes().to_vec(),
            exit_code: 2,
        };
    }

    // Load config.
    let config = if let Some(c) = config_override {
        c
    } else {
        match AskConfig::load_or_default() {
            Ok(c) => c,
            Err(e) => {
                return AskOutput {
                    stdout: String::new(),
                    stderr: format!("clank: ask: {e}\n").into_bytes(),
                    exit_code: 1,
                };
            }
        }
    };

    let model = config.resolve_model(flags.model.as_deref());

    // Select the provider and resolve its API key.
    let provider: Box<dyn ModelProvider> = match select_provider(&model, &config, Arc::clone(&http))
    {
        Ok(p) => p,
        Err(e) => {
            return AskOutput {
                stdout: String::new(),
                stderr: format!("clank: ask: {e}\n").into_bytes(),
                exit_code: 1,
            };
        }
    };

    // Build system prompt, embedding the transcript as context so the model
    // has session history without requiring fake conversation turns.
    let system_prompt = build_system_prompt(
        cwd,
        if flags.fresh {
            None
        } else {
            Some(&transcript_text)
        },
    );

    // Build messages. We construct a single user message that may be prefixed
    // with piped stdin.
    let user_content = if !piped_stdin.is_empty() {
        let stdin_text = String::from_utf8_lossy(&piped_stdin);
        format!("[Supplementary input]\n{stdin_text}\n\n{}", flags.prompt)
    } else {
        flags.prompt.clone()
    };

    let messages = vec![Message {
        role: Role::User,
        content: user_content,
    }];

    let request = CompletionRequest {
        model: model.clone(),
        system_prompt,
        messages,
    };

    match provider.as_ref().complete(request).await {
        Ok(resp) => {
            let content = resp.content;

            if flags.json {
                // Validate that the response is valid JSON.
                match serde_json::from_str::<serde_json::Value>(&content) {
                    Ok(_) => AskOutput {
                        stdout: content + "\n",
                        stderr: Vec::new(),
                        exit_code: 0,
                    },
                    Err(_) => {
                        let stderr = format!("clank: ask: response is not valid JSON\n{content}\n");
                        AskOutput {
                            stdout: String::new(),
                            stderr: stderr.into_bytes(),
                            exit_code: 6,
                        }
                    }
                }
            } else {
                let output = if content.ends_with('\n') {
                    content
                } else {
                    content + "\n"
                };
                AskOutput {
                    stdout: output,
                    stderr: Vec::new(),
                    exit_code: 0,
                }
            }
        }
        Err(ProviderError::Timeout) => AskOutput {
            stdout: String::new(),
            stderr: b"clank: ask: model request timed out\n".to_vec(),
            exit_code: 3,
        },
        Err(e) => AskOutput {
            stdout: String::new(),
            stderr: format!("clank: ask: {e}\n").into_bytes(),
            exit_code: e.exit_code(),
        },
    }
}

/// Select the appropriate model provider based on the model name and config.
///
/// Selection logic:
/// 1. If the provider prefix is `"ollama"` or `"openai-compat"`, handle as a
///    local provider (no API key required).
/// 2. If a direct API key exists for the named provider, use it directly.
/// 3. Otherwise, fall back to OpenRouter if an openrouter key is configured.
/// 4. If neither is available, return an informative error.
fn select_provider(
    model: &str,
    config: &AskConfig,
    http: Arc<dyn HttpClient>,
) -> Result<Box<dyn ModelProvider>, ProviderSelectError> {
    let provider_name = if model.contains('/') {
        model.split('/').next().unwrap_or("anthropic")
    } else {
        "anthropic"
    };

    // Local providers — no API key required; handled before the key-based check.
    match provider_name {
        "ollama" => {
            let base_url = config
                .base_url("ollama")
                .unwrap_or("http://localhost:11434")
                .to_string();
            return Ok(Box::new(OllamaProvider::new(base_url, http)));
        }
        "openai-compat" => {
            let base_url = config
                .base_url("openai-compat")
                .ok_or(ProviderSelectError::MissingBaseUrl)?;
            let api_key = config.api_key("openai-compat").map(str::to_string);
            return Ok(Box::new(OpenAiCompatProvider::new(
                base_url.to_string(),
                api_key,
                http,
            )));
        }
        _ => {}
    }

    // Cloud providers — require a direct API key or OpenRouter fallback.
    if let Some(key) = config.api_key(provider_name) {
        return Ok(match provider_name {
            "anthropic" => Box::new(AnthropicProvider::new(key, http)),
            "openrouter" => Box::new(OpenRouterProvider::new(key, http)),
            // Any other provider with a direct key configured also routes through
            // OpenRouter, which accepts all provider/model strings natively.
            _ => Box::new(OpenRouterProvider::new(key, http)),
        });
    }

    // OpenRouter fallback for any model without a direct provider key.
    if let Some(key) = config.api_key("openrouter") {
        return Ok(Box::new(OpenRouterProvider::new(key, http)));
    }

    // Neither configured.
    Err(ProviderSelectError::MissingApiKey {
        provider_name: provider_name.to_string(),
    })
}

fn build_system_prompt(cwd: Option<&str>, transcript: Option<&str>) -> String {
    let cwd_str = cwd.unwrap_or("unknown");
    let mut prompt = format!(
        "You are a shell session assistant. You have been given a transcript of the \
user's current shell session — the commands they have run and the output those \
commands produced.\n\
\n\
Answer the user's question using only the information visible in the session \
transcript below. Do not suggest commands to run, do not describe steps you would \
take, and do not speculate about files or system state that does not appear in \
the transcript.\n\
\n\
Working directory: {cwd_str}\n"
    );

    // Embed the session transcript as context so the model can see what the
    // user has been doing, without fabricating conversation turns.
    if let Some(t) = transcript {
        if !t.trim().is_empty() {
            prompt.push_str("\n## Session transcript\n");
            prompt.push_str(t);
        }
    }

    prompt
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use clank_http::{MockHttpClient, MockResponse};

    use crate::config::ProviderConfig;

    use super::*;

    fn mock_config() -> AskConfig {
        let mut providers = std::collections::HashMap::new();
        providers.insert(
            "anthropic".to_string(),
            ProviderConfig {
                api_key: Some("sk-test-key".to_string()),
                base_url: None,
            },
        );
        AskConfig {
            default_model: Some("anthropic/claude-sonnet-4-5".to_string()),
            providers,
        }
    }

    fn mock_success_response(text: &str) -> MockResponse {
        let body = serde_json::json!({
            "content": [{ "type": "text", "text": text }],
            "id": "msg_test", "model": "claude-sonnet-4-5",
            "role": "assistant", "stop_reason": "end_turn",
            "type": "message",
            "usage": { "input_tokens": 5, "output_tokens": 5 }
        });
        MockResponse::json(body.to_string())
    }

    #[tokio::test]
    async fn test_ask_fresh_ignores_transcript() {
        let mock = Arc::new(MockHttpClient::new(vec![mock_success_response("ok")]));
        let AskOutput {
            stdout: out,
            stderr: err,
            exit_code: code,
        } = run_ask(
            &["ask".into(), "--fresh".into(), "hello".into()],
            Vec::new(),
            Arc::clone(&mock) as Arc<dyn HttpClient>,
            "$ ls\nfoo bar\n".to_string(),
            Some("/tmp"),
            Some(mock_config()),
        )
        .await;
        assert_eq!(code, 0, "stderr: {}", String::from_utf8_lossy(&err));
        assert!(out.contains("ok"));

        // With --fresh, the transcript must NOT appear in the system prompt.
        let reqs = mock.requests.lock().await;
        let body: serde_json::Value =
            serde_json::from_slice(reqs[0].body.as_deref().unwrap()).unwrap();
        let system = body["system"].as_str().unwrap_or("");
        assert!(
            !system.contains("Session transcript"),
            "fresh should not include transcript in system prompt"
        );
        // The system prompt must not contain agentic tool-calling language.
        assert!(
            !system.contains("Available tools"),
            "system prompt must not reference available tools in Phase 1"
        );
        assert!(
            !system.contains("executing commands"),
            "system prompt must not instruct the model to execute commands in Phase 1"
        );
        // No fake assistant turns.
        let messages = body["messages"].as_array().unwrap();
        assert!(
            messages.iter().all(|m| m["role"] != "assistant"),
            "no fake assistant turns should be present"
        );
    }

    #[tokio::test]
    async fn test_ask_inherit_includes_transcript() {
        let mock = Arc::new(MockHttpClient::new(vec![mock_success_response("ok")]));
        let AskOutput {
            stderr: err,
            exit_code: code,
            ..
        } = run_ask(
            &["ask".into(), "--inherit".into(), "hello".into()],
            Vec::new(),
            Arc::clone(&mock) as Arc<dyn HttpClient>,
            "$ ls\nfoo bar\n".to_string(),
            Some("/tmp"),
            Some(mock_config()),
        )
        .await;
        assert_eq!(code, 0, "stderr: {}", String::from_utf8_lossy(&err));

        // Transcript is embedded in the system prompt, not in messages.
        let reqs = mock.requests.lock().await;
        let body: serde_json::Value =
            serde_json::from_slice(reqs[0].body.as_deref().unwrap()).unwrap();
        let system = body["system"].as_str().unwrap_or("");
        assert!(
            system.contains("Session transcript"),
            "system prompt should contain transcript"
        );
        assert!(
            system.contains("$ ls"),
            "transcript content should be in system prompt"
        );
        // The system prompt must not contain agentic tool-calling language.
        assert!(
            !system.contains("Available tools"),
            "system prompt must not reference available tools in Phase 1"
        );
        assert!(
            !system.contains("executing commands"),
            "system prompt must not instruct the model to execute commands in Phase 1"
        );
        // No fake assistant turns.
        let messages = body["messages"].as_array().unwrap();
        assert!(
            messages.iter().all(|m| m["role"] != "assistant"),
            "no fake assistant turns should be present"
        );
    }

    #[tokio::test]
    async fn test_no_fake_assistant_turns_in_request() {
        // Regardless of transcript or piped stdin, no fake assistant turns.
        let mock = Arc::new(MockHttpClient::new(vec![mock_success_response("ok")]));
        let AskOutput { .. } = run_ask(
            &["ask".into(), "hello".into()],
            b"some piped data".to_vec(),
            Arc::clone(&mock) as Arc<dyn HttpClient>,
            "$ echo hi\nhi\n".to_string(),
            Some("/tmp"),
            Some(mock_config()),
        )
        .await;
        let reqs = mock.requests.lock().await;
        let body: serde_json::Value =
            serde_json::from_slice(reqs[0].body.as_deref().unwrap()).unwrap();
        let messages = body["messages"].as_array().unwrap();
        assert!(
            messages.iter().all(|m| m["role"] != "assistant"),
            "no fake assistant turns should appear in messages"
        );
        // Exactly one user message.
        assert_eq!(messages.len(), 1, "should be exactly one user message");
    }

    #[tokio::test]
    async fn test_ask_json_valid_exits_0() {
        let mock = Arc::new(MockHttpClient::new(vec![mock_success_response(
            r#"{"answer": 42}"#,
        )]));
        let AskOutput {
            stdout: out,
            exit_code: code,
            ..
        } = run_ask(
            &["ask".into(), "--json".into(), "give json".into()],
            Vec::new(),
            Arc::clone(&mock) as Arc<dyn HttpClient>,
            String::new(),
            None,
            Some(mock_config()),
        )
        .await;
        assert_eq!(code, 0);
        assert!(out.contains("42"));
    }

    #[tokio::test]
    async fn test_ask_json_invalid_exits_6_stderr_has_raw() {
        let mock = Arc::new(MockHttpClient::new(vec![mock_success_response(
            "not json at all",
        )]));
        let AskOutput {
            stdout: out,
            stderr: err,
            exit_code: code,
        } = run_ask(
            &["ask".into(), "--json".into(), "give json".into()],
            Vec::new(),
            Arc::clone(&mock) as Arc<dyn HttpClient>,
            String::new(),
            None,
            Some(mock_config()),
        )
        .await;
        assert_eq!(code, 6);
        assert!(out.is_empty());
        let err_str = String::from_utf8_lossy(&err);
        assert!(
            err_str.contains("not json at all"),
            "raw response missing from stderr: {err_str}"
        );
    }

    #[tokio::test]
    async fn test_ask_piped_stdin_appended_to_context() {
        let mock = Arc::new(MockHttpClient::new(vec![mock_success_response("ok")]));
        let AskOutput {
            stderr: err,
            exit_code: code,
            ..
        } = run_ask(
            &["ask".into(), "summarize this".into()],
            b"some piped content".to_vec(),
            Arc::clone(&mock) as Arc<dyn HttpClient>,
            String::new(),
            Some("/tmp"),
            Some(mock_config()),
        )
        .await;
        assert_eq!(code, 0, "stderr: {}", String::from_utf8_lossy(&err));

        let reqs = mock.requests.lock().await;
        let body: serde_json::Value =
            serde_json::from_slice(reqs[0].body.as_deref().unwrap()).unwrap();
        let messages = body["messages"].as_array().unwrap();
        let has_stdin = messages.iter().any(|m| {
            m["content"]
                .as_str()
                .unwrap_or("")
                .contains("some piped content")
        });
        assert!(has_stdin, "piped stdin missing from messages");
    }

    #[tokio::test]
    async fn test_ask_bad_flag_exits_2() {
        let mock = Arc::new(MockHttpClient::new(vec![]));
        let AskOutput {
            stderr: err,
            exit_code: code,
            ..
        } = run_ask(
            &["ask".into(), "--unknown-flag".into()],
            Vec::new(),
            mock as Arc<dyn HttpClient>,
            String::new(),
            None,
            None,
        )
        .await;
        assert_eq!(code, 2);
        assert!(!err.is_empty());
    }

    #[tokio::test]
    async fn test_ask_empty_prompt_exits_2() {
        let mock = Arc::new(MockHttpClient::new(vec![]));
        let AskOutput {
            stderr: err,
            exit_code: code,
            ..
        } = run_ask(
            &["ask".into()],
            Vec::new(),
            mock as Arc<dyn HttpClient>,
            String::new(),
            None,
            None,
        )
        .await;
        assert_eq!(code, 2);
        assert!(!err.is_empty());
    }

    #[tokio::test]
    async fn test_ask_no_config_exits_1() {
        let mock = Arc::new(MockHttpClient::new(vec![]));
        let AskOutput {
            stderr: err,
            exit_code: code,
            ..
        } = run_ask(
            &["ask".into(), "hello".into()],
            Vec::new(),
            mock as Arc<dyn HttpClient>,
            String::new(),
            None,
            // Pass an explicit empty config so this test is not affected by any
            // CLANK_CONFIG env var set by a concurrently running test.
            Some(AskConfig::default()),
        )
        .await;
        assert!(code != 0, "should fail without config");
        assert!(!err.is_empty(), "should emit stderr message");
    }

    // -----------------------------------------------------------------------
    // Provider selection tests — cloud providers
    // -----------------------------------------------------------------------

    fn anthropic_config() -> AskConfig {
        let mut providers = std::collections::HashMap::new();
        providers.insert(
            "anthropic".to_string(),
            ProviderConfig {
                api_key: Some("sk-ant-test".to_string()),
                base_url: None,
            },
        );
        AskConfig {
            default_model: Some("anthropic/claude-sonnet-4-5".to_string()),
            providers,
        }
    }

    fn openrouter_config() -> AskConfig {
        let mut providers = std::collections::HashMap::new();
        providers.insert(
            "openrouter".to_string(),
            ProviderConfig {
                api_key: Some("sk-or-test".to_string()),
                base_url: None,
            },
        );
        AskConfig {
            default_model: Some("anthropic/claude-sonnet-4-5".to_string()),
            providers,
        }
    }

    fn both_config() -> AskConfig {
        let mut providers = std::collections::HashMap::new();
        providers.insert(
            "anthropic".to_string(),
            ProviderConfig {
                api_key: Some("sk-ant-direct".to_string()),
                base_url: None,
            },
        );
        providers.insert(
            "openrouter".to_string(),
            ProviderConfig {
                api_key: Some("sk-or-test".to_string()),
                base_url: None,
            },
        );
        AskConfig {
            default_model: Some("anthropic/claude-sonnet-4-5".to_string()),
            providers,
        }
    }

    fn or_success_response(text: &str) -> MockResponse {
        // OpenRouter uses choices[0].message.content
        let body = serde_json::json!({
            "id": "gen-test",
            "choices": [{"message": {"role": "assistant", "content": text}}],
            "model": "anthropic/claude-sonnet-4-5",
            "usage": {"prompt_tokens": 5, "completion_tokens": 5, "total_tokens": 10}
        });
        MockResponse::json(body.to_string())
    }

    #[tokio::test]
    async fn test_provider_selection_anthropic_direct() {
        // Anthropic key present → direct Anthropic provider.
        // Anthropic uses content[0].text in its response format.
        let mock = Arc::new(MockHttpClient::new(vec![mock_success_response("direct")]));
        let AskOutput {
            stdout: out,
            stderr: err,
            exit_code: code,
        } = run_ask(
            &["ask".into(), "hello".into()],
            Vec::new(),
            Arc::clone(&mock) as Arc<dyn HttpClient>,
            String::new(),
            None,
            Some(anthropic_config()),
        )
        .await;
        assert_eq!(code, 0, "stderr: {}", String::from_utf8_lossy(&err));
        assert!(out.contains("direct"));

        // Request went to Anthropic endpoint
        let reqs = mock.requests.lock().await;
        assert!(reqs[0].url.contains("anthropic.com"));
    }

    #[tokio::test]
    async fn test_provider_selection_openrouter_fallback() {
        // No anthropic key, openrouter key present → OpenRouter provider.
        let mock = Arc::new(MockHttpClient::new(vec![or_success_response("via-or")]));
        let AskOutput {
            stdout: out,
            stderr: err,
            exit_code: code,
        } = run_ask(
            &["ask".into(), "hello".into()],
            Vec::new(),
            Arc::clone(&mock) as Arc<dyn HttpClient>,
            String::new(),
            None,
            Some(openrouter_config()),
        )
        .await;
        assert_eq!(code, 0, "stderr: {}", String::from_utf8_lossy(&err));
        assert!(out.contains("via-or"));

        // Request went to OpenRouter endpoint
        let reqs = mock.requests.lock().await;
        assert!(reqs[0].url.contains("openrouter.ai"));
    }

    #[tokio::test]
    async fn test_provider_selection_direct_takes_precedence() {
        // Both keys present → direct anthropic key takes precedence.
        let mock = Arc::new(MockHttpClient::new(vec![mock_success_response("direct")]));
        let AskOutput {
            stdout: out,
            stderr: err,
            exit_code: code,
        } = run_ask(
            &["ask".into(), "hello".into()],
            Vec::new(),
            Arc::clone(&mock) as Arc<dyn HttpClient>,
            String::new(),
            None,
            Some(both_config()),
        )
        .await;
        assert_eq!(code, 0, "stderr: {}", String::from_utf8_lossy(&err));
        assert!(out.contains("direct"));

        let reqs = mock.requests.lock().await;
        assert!(
            reqs[0].url.contains("anthropic.com"),
            "direct anthropic should take precedence over openrouter"
        );
    }

    #[tokio::test]
    async fn test_provider_selection_no_key_exits_1() {
        let mock = Arc::new(MockHttpClient::new(vec![]));
        let AskOutput {
            stderr: err,
            exit_code: code,
            ..
        } = run_ask(
            &["ask".into(), "hello".into()],
            Vec::new(),
            mock as Arc<dyn HttpClient>,
            String::new(),
            None,
            Some(AskConfig::default()),
        )
        .await;
        assert_eq!(code, 1);
        let err_str = String::from_utf8_lossy(&err);
        assert!(
            err_str.contains("no API key"),
            "should explain that no key is configured"
        );
        assert!(
            err_str.contains("openrouter"),
            "should mention openrouter as an option"
        );
    }

    #[tokio::test]
    async fn test_provider_selection_openrouter_with_anthropic_model() {
        // OpenRouter key + anthropic/model → full model string passed through.
        let mock = Arc::new(MockHttpClient::new(vec![or_success_response("ok")]));
        let _ = run_ask(
            &["ask".into(), "hello".into()],
            Vec::new(),
            Arc::clone(&mock) as Arc<dyn HttpClient>,
            String::new(),
            None,
            Some(openrouter_config()),
        )
        .await;

        let reqs = mock.requests.lock().await;
        let body: serde_json::Value =
            serde_json::from_slice(reqs[0].body.as_deref().unwrap()).unwrap();
        assert_eq!(
            body["model"], "anthropic/claude-sonnet-4-5",
            "OpenRouter must receive the full model string unmodified"
        );
    }

    // -----------------------------------------------------------------------
    // Provider selection tests — local providers
    // -----------------------------------------------------------------------

    fn ollama_config() -> AskConfig {
        let mut providers = std::collections::HashMap::new();
        providers.insert(
            "ollama".to_string(),
            ProviderConfig {
                api_key: None,
                base_url: Some("http://localhost:11434".to_string()),
            },
        );
        AskConfig {
            default_model: Some("ollama/llama3.2".to_string()),
            providers,
        }
    }

    fn ollama_custom_url_config() -> AskConfig {
        let mut providers = std::collections::HashMap::new();
        providers.insert(
            "ollama".to_string(),
            ProviderConfig {
                api_key: None,
                base_url: Some("http://myhost:11434".to_string()),
            },
        );
        AskConfig {
            default_model: Some("ollama/llama3.2".to_string()),
            providers,
        }
    }

    fn openai_compat_config() -> AskConfig {
        let mut providers = std::collections::HashMap::new();
        providers.insert(
            "openai-compat".to_string(),
            ProviderConfig {
                api_key: None,
                base_url: Some("http://localhost:8080".to_string()),
            },
        );
        AskConfig {
            default_model: Some("openai-compat/phi4".to_string()),
            providers,
        }
    }

    fn ollama_success_response(text: &str) -> MockResponse {
        let body = serde_json::json!({
            "model": "llama3.2",
            "message": { "role": "assistant", "content": text },
            "done": true
        });
        MockResponse::json(body.to_string())
    }

    #[tokio::test]
    async fn test_select_ollama_no_base_url_uses_default() {
        // Config has providers.ollama with no base_url → default must be used.
        let mut providers = std::collections::HashMap::new();
        providers.insert(
            "ollama".to_string(),
            ProviderConfig {
                api_key: None,
                base_url: None,
            },
        );
        let config = AskConfig {
            default_model: Some("ollama/llama3.2".to_string()),
            providers,
        };

        let mock = Arc::new(MockHttpClient::new(vec![ollama_success_response("ok")]));
        let AskOutput {
            stderr: err,
            exit_code: code,
            ..
        } = run_ask(
            &[
                "ask".into(),
                "--model".into(),
                "ollama/llama3.2".into(),
                "hi".into(),
            ],
            Vec::new(),
            Arc::clone(&mock) as Arc<dyn HttpClient>,
            String::new(),
            None,
            Some(config),
        )
        .await;
        assert_eq!(code, 0, "stderr: {}", String::from_utf8_lossy(&err));

        let reqs = mock.requests.lock().await;
        assert!(
            reqs[0].url.starts_with("http://localhost:11434"),
            "expected default ollama URL, got: {}",
            reqs[0].url
        );
    }

    #[tokio::test]
    async fn test_select_ollama_custom_base_url() {
        let mock = Arc::new(MockHttpClient::new(vec![ollama_success_response("ok")]));
        let AskOutput {
            stderr: err,
            exit_code: code,
            ..
        } = run_ask(
            &[
                "ask".into(),
                "--model".into(),
                "ollama/llama3.2".into(),
                "hi".into(),
            ],
            Vec::new(),
            Arc::clone(&mock) as Arc<dyn HttpClient>,
            String::new(),
            None,
            Some(ollama_custom_url_config()),
        )
        .await;
        assert_eq!(code, 0, "stderr: {}", String::from_utf8_lossy(&err));

        let reqs = mock.requests.lock().await;
        assert!(
            reqs[0].url.starts_with("http://myhost:11434"),
            "expected custom ollama URL, got: {}",
            reqs[0].url
        );
    }

    #[test]
    fn test_select_openai_compat_missing_url_returns_error() {
        // providers.openai-compat exists but has no base_url → typed error.
        let mut providers = std::collections::HashMap::new();
        providers.insert(
            "openai-compat".to_string(),
            ProviderConfig {
                api_key: None,
                base_url: None,
            },
        );
        let config = AskConfig {
            default_model: Some("openai-compat/phi4".to_string()),
            providers,
        };

        let mock = Arc::new(MockHttpClient::new(vec![]));
        let result = select_provider("openai-compat/phi4", &config, mock as Arc<dyn HttpClient>);
        assert!(result.is_err(), "expected error from select_provider");
        let err = result.err().unwrap();
        assert!(
            matches!(err, ProviderSelectError::MissingBaseUrl),
            "expected MissingBaseUrl variant, got: {err:?}"
        );
        // The Display output must contain user-actionable hint.
        let display = err.to_string();
        assert!(
            display.contains("no base_url"),
            "expected 'no base_url' in error display: {display}"
        );
        assert!(
            display.contains("model add openai-compat"),
            "expected model add hint in error display: {display}"
        );
    }

    #[tokio::test]
    async fn test_select_openai_compat_missing_url_run_ask_exits_1() {
        // Full run_ask path: openai-compat model with no base_url → exit 1 with hint.
        let mut providers = std::collections::HashMap::new();
        providers.insert(
            "openai-compat".to_string(),
            ProviderConfig {
                api_key: None,
                base_url: None,
            },
        );
        let config = AskConfig {
            default_model: Some("openai-compat/phi4".to_string()),
            providers,
        };

        let mock = Arc::new(MockHttpClient::new(vec![]));
        let AskOutput {
            stderr: err,
            exit_code: code,
            ..
        } = run_ask(
            &[
                "ask".into(),
                "--model".into(),
                "openai-compat/phi4".into(),
                "hi".into(),
            ],
            Vec::new(),
            mock as Arc<dyn HttpClient>,
            String::new(),
            None,
            Some(config),
        )
        .await;
        assert_eq!(code, 1);
        let err_str = String::from_utf8_lossy(&err);
        assert!(
            err_str.contains("no base_url"),
            "expected 'no base_url' in stderr: {err_str}"
        );
    }

    #[tokio::test]
    async fn test_select_provider_ollama_routing() {
        // run_ask with ollama model → request hits /api/chat.
        let mock = Arc::new(MockHttpClient::new(vec![ollama_success_response("hi")]));
        let AskOutput {
            stderr: err,
            exit_code: code,
            ..
        } = run_ask(
            &[
                "ask".into(),
                "--model".into(),
                "ollama/llama3.2".into(),
                "hi".into(),
            ],
            Vec::new(),
            Arc::clone(&mock) as Arc<dyn HttpClient>,
            String::new(),
            None,
            Some(ollama_config()),
        )
        .await;
        assert_eq!(code, 0, "stderr: {}", String::from_utf8_lossy(&err));

        let reqs = mock.requests.lock().await;
        assert!(
            reqs[0].url.ends_with("/api/chat"),
            "expected /api/chat, got: {}",
            reqs[0].url
        );
    }

    #[tokio::test]
    async fn test_select_provider_openai_compat_routing() {
        // run_ask with openai-compat model → request hits /v1/chat/completions.
        let mock = Arc::new(MockHttpClient::new(vec![or_success_response("hi")]));
        let AskOutput {
            stderr: err,
            exit_code: code,
            ..
        } = run_ask(
            &[
                "ask".into(),
                "--model".into(),
                "openai-compat/phi4".into(),
                "hi".into(),
            ],
            Vec::new(),
            Arc::clone(&mock) as Arc<dyn HttpClient>,
            String::new(),
            None,
            Some(openai_compat_config()),
        )
        .await;
        assert_eq!(code, 0, "stderr: {}", String::from_utf8_lossy(&err));

        let reqs = mock.requests.lock().await;
        assert!(
            reqs[0].url.ends_with("/v1/chat/completions"),
            "expected /v1/chat/completions, got: {}",
            reqs[0].url
        );
    }
}
