use std::sync::{Arc, RwLock};

use async_trait::async_trait;
use clank_ask::config::{AskConfig, DEFAULT_MODEL};
use clank_ask::provider::anthropic::AnthropicProvider;
use clank_ask::provider::{CompletionRequest, Message, ModelProvider, ProviderError, Role};
use clank_http::HttpClient;

use crate::process::{Process, ProcessContext, ProcessResult};
use crate::transcript::Transcript;

/// Implementation of the `context` shell-internal builtin.
pub struct ContextProcess {
    transcript: Arc<RwLock<Transcript>>,
    http: Arc<dyn HttpClient>,
    /// Optional config override for testing.
    config_override: Option<AskConfig>,
}

impl ContextProcess {
    pub fn new(transcript: Arc<RwLock<Transcript>>, http: Arc<dyn HttpClient>) -> Self {
        Self {
            transcript,
            http,
            config_override: None,
        }
    }

    /// Create with an injected config (for testing).
    pub fn with_config(
        transcript: Arc<RwLock<Transcript>>,
        http: Arc<dyn HttpClient>,
        config: AskConfig,
    ) -> Self {
        Self {
            transcript,
            http,
            config_override: Some(config),
        }
    }
}

#[async_trait]
impl Process for ContextProcess {
    async fn run(&self, mut ctx: ProcessContext) -> ProcessResult {
        let subcommand = ctx.argv.get(1).map(String::as_str).unwrap_or("");

        match subcommand {
            "show" => {
                let t = self.transcript.read().expect("transcript lock poisoned");
                let output = t.format_full();
                if output.is_empty() {
                    let _ = ctx.io.write_stdout(b"(transcript is empty)\n");
                } else {
                    let _ = ctx.io.write_stdout(output.as_bytes());
                }
                ProcessResult::success()
            }

            "clear" => {
                self.transcript
                    .write()
                    .expect("transcript lock poisoned")
                    .clear();
                ProcessResult::success()
            }

            "summarize" => {
                let transcript_text = {
                    let t = self.transcript.read().expect("transcript lock poisoned");
                    t.format_full()
                };

                if transcript_text.trim().is_empty() {
                    let _ = ctx.io.write_stdout(b"(transcript is empty)\n");
                    return ProcessResult::success();
                }

                // Load or use injected config.
                let config = match self
                    .config_override
                    .clone()
                    .map(Ok)
                    .unwrap_or_else(|| AskConfig::load_or_default().map_err(|e| e.to_string()))
                {
                    Ok(c) => c,
                    Err(msg) => {
                        let _ = ctx
                            .io
                            .write_stderr(format!("clank: context summarize: {msg}\n").as_bytes());
                        return ProcessResult::failure(1);
                    }
                };

                // Extract API key — prefer anthropic, fall back to openrouter.
                let api_key = match config
                    .api_key("anthropic")
                    .or_else(|| config.api_key("openrouter"))
                    .map(str::to_string)
                {
                    Some(k) => k,
                    None => {
                        let _ = ctx.io.write_stderr(
                            b"clank: context summarize: no API key configured\n\
                              Run: model add anthropic --key <KEY>\n",
                        );
                        return ProcessResult::failure(1);
                    }
                };

                // Build the provider using the injected HTTP client.
                let provider = AnthropicProvider::new(api_key, Arc::clone(&self.http));

                let model = config.default_model.as_deref().unwrap_or(DEFAULT_MODEL);

                let request = CompletionRequest {
                    model: model.to_string(),
                    system_prompt: "You are a concise summarizer. Summarize the following shell \
                                    session transcript in a few sentences, focusing on what was \
                                    done and what was found."
                        .to_string(),
                    messages: vec![Message {
                        role: Role::User,
                        content: transcript_text,
                    }],
                };

                match provider.complete(request).await {
                    Ok(resp) => {
                        let output = if resp.content.ends_with('\n') {
                            resp.content
                        } else {
                            resp.content + "\n"
                        };
                        let _ = ctx.io.write_stdout(output.as_bytes());
                        ProcessResult::success()
                    }
                    Err(ProviderError::Timeout) => {
                        let _ = ctx
                            .io
                            .write_stderr(b"clank: context summarize: model request timed out\n");
                        ProcessResult::failure(3)
                    }
                    Err(e) => {
                        let _ = ctx
                            .io
                            .write_stderr(format!("clank: context summarize: {e}\n").as_bytes());
                        ProcessResult::failure(e.exit_code())
                    }
                }
            }

            "trim" => {
                let n_str = ctx.argv.get(2).map(String::as_str).unwrap_or("");
                match n_str.parse::<usize>() {
                    Ok(n) => {
                        self.transcript
                            .write()
                            .expect("transcript lock poisoned")
                            .trim(n);
                        ProcessResult::success()
                    }
                    Err(_) => {
                        let _ = ctx.io.write_stderr(
                            format!(
                                "clank: context trim: expected a non-negative integer, got {:?}\n",
                                n_str
                            )
                            .as_bytes(),
                        );
                        ProcessResult::failure(2)
                    }
                }
            }

            "" => {
                let _ = ctx
                    .io
                    .write_stderr(b"usage: context <show|clear|summarize|trim <n>>\n");
                ProcessResult::failure(2)
            }

            other => {
                let _ = ctx.io.write_stderr(
                    format!("clank: context: unknown subcommand: {other}\n").as_bytes(),
                );
                ProcessResult::failure(2)
            }
        }
    }
}
