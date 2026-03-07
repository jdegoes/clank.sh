/// `ask` — invoke the configured AI model with the current transcript as context.
///
/// `ask` is `shell-internal` scoped: it is intercepted in the REPL, reads from
/// and writes to `ClankShell`'s transcript, and calls the model via `clank-http`.
use std::sync::Arc;

use clank_http::{HttpClient, HttpError, RequestHeader};
use serde_json::{Value, json};
use thiserror::Error;

const ANTHROPIC_URL: &str = "https://api.anthropic.com/v1/messages";
const ANTHROPIC_VERSION: &str = "2023-06-01";
const MAX_TOKENS: u32 = 4096;

const SYSTEM_PROMPT: &str =
    "You are an AI agent operating a bash-compatible shell called clank.sh. \
     The transcript shows the session history. Respond helpfully and concisely.";

// ── Error type ────────────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum AskError {
    #[error("no default model configured — run: model add <provider> --key <key>")]
    NoModelConfigured,
    #[error("no API key for provider '{0}' — run: model add {0} --key <key>")]
    NoApiKey(String),
    #[error("HTTP request failed: {0}")]
    Http(#[from] HttpError),
    #[error("unexpected API response: {0}")]
    UnexpectedResponse(String),
    #[error("failed to parse ask invocation: {0}")]
    ParseError(String),
    #[error("failed to load config: {0}")]
    Config(String),
}

// ── Parsed invocation ─────────────────────────────────────────────────────────

/// A parsed `ask` invocation from the REPL input line.
pub struct AskInvocation {
    /// The user's prompt text.
    pub prompt: String,
    /// If true, send only the prompt — no transcript context.
    pub fresh: bool,
    /// Model override. If `None`, use the configured default.
    pub model_override: Option<String>,
}

impl AskInvocation {
    /// Parse an `ask` invocation from a raw REPL input string.
    ///
    /// Supported forms:
    /// - `ask "prompt"`
    /// - `ask --fresh "prompt"`
    /// - `ask --model anthropic/claude-sonnet-4-5 "prompt"`
    /// - `ask --fresh --model anthropic/claude-sonnet-4-5 "prompt"`
    pub fn parse(input: &str) -> Result<Self, AskError> {
        // Strip leading `ask` keyword.
        let rest = input
            .trim()
            .strip_prefix("ask")
            .unwrap_or(input)
            .trim()
            .to_string();

        let mut fresh = false;
        let mut model_override: Option<String> = None;
        let mut remaining = rest.as_str();

        loop {
            if remaining.starts_with("--fresh") {
                fresh = true;
                remaining = remaining["--fresh".len()..].trim_start();
            } else if remaining.starts_with("--no-transcript") {
                fresh = true;
                remaining = remaining["--no-transcript".len()..].trim_start();
            } else if remaining.starts_with("--model") {
                remaining = remaining["--model".len()..].trim_start();
                // Next token is the model name.
                let (model, rest_after) = split_first_token(remaining);
                if model.is_empty() {
                    return Err(AskError::ParseError(
                        "--model requires a model name".to_string(),
                    ));
                }
                model_override = Some(model.to_string());
                remaining = rest_after.trim_start();
            } else {
                break;
            }
        }

        // Strip surrounding quotes from the prompt if present.
        let prompt = strip_quotes(remaining).to_string();

        if prompt.is_empty() {
            return Err(AskError::ParseError("ask requires a prompt".to_string()));
        }

        Ok(Self {
            prompt,
            fresh,
            model_override,
        })
    }
}

// ── Request construction ──────────────────────────────────────────────────────

/// Build the user message content from transcript context and the prompt.
pub fn build_user_content(transcript_context: &str, prompt: &str) -> String {
    if transcript_context.is_empty() {
        prompt.to_string()
    } else {
        format!("{transcript_context}\n\n{prompt}")
    }
}

/// Build the Anthropic Messages API request body.
pub fn build_request_body(model: &str, user_content: &str) -> String {
    json!({
        "model": model,
        "max_tokens": MAX_TOKENS,
        "system": SYSTEM_PROMPT,
        "messages": [
            { "role": "user", "content": user_content }
        ]
    })
    .to_string()
}

/// Strip the provider prefix from a model name.
/// `"anthropic/claude-sonnet-4-5"` → `"claude-sonnet-4-5"`.
pub fn strip_provider_prefix(model: &str) -> &str {
    model.split_once('/').map(|(_, m)| m).unwrap_or(model)
}

// ── Response parsing ──────────────────────────────────────────────────────────

/// Extract the text response from an Anthropic Messages API JSON response body.
pub fn extract_response_text(body: &str) -> Result<String, AskError> {
    let value: Value = serde_json::from_str(body)
        .map_err(|e| AskError::UnexpectedResponse(format!("invalid JSON: {e}")))?;

    // Check for API-level error responses.
    if let Some(error) = value.get("error") {
        let msg = error
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("unknown error");
        return Err(AskError::UnexpectedResponse(format!("API error: {msg}")));
    }

    value
        .get("content")
        .and_then(|c| c.as_array())
        .and_then(|arr| arr.first())
        .and_then(|block| block.get("text"))
        .and_then(|t| t.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| {
            AskError::UnexpectedResponse(format!(
                "no text content in response: {body}"
            ))
        })
}

// ── Main ask execution ────────────────────────────────────────────────────────

/// Execute an `ask` invocation against the Anthropic API.
///
/// Returns the model's response text on success. The caller is responsible
/// for appending the response to the transcript.
pub async fn execute(
    invocation: &AskInvocation,
    transcript_context: &str,
    http: &Arc<dyn HttpClient>,
) -> Result<String, AskError> {
    // Load config.
    let config = clank_config::load_config()
        .map_err(|e| AskError::Config(e.to_string()))?;

    // Resolve model name.
    let configured_model = config
        .default_model
        .as_deref()
        .or(invocation.model_override.as_deref())
        .ok_or(AskError::NoModelConfigured)?;

    let model = invocation
        .model_override
        .as_deref()
        .unwrap_or(configured_model);

    let model_for_api = strip_provider_prefix(model).to_string();

    // Resolve API key.
    let api_key = config
        .api_key_for_model(model)
        .ok_or_else(|| {
            let provider = model.split('/').next().unwrap_or(model).to_string();
            AskError::NoApiKey(provider)
        })?
        .to_string();

    // Build request.
    let context = if invocation.fresh { "" } else { transcript_context };
    let user_content = build_user_content(context, &invocation.prompt);
    let body = build_request_body(&model_for_api, &user_content);

    let headers = vec![
        RequestHeader::new("x-api-key", &api_key),
        RequestHeader::new("anthropic-version", ANTHROPIC_VERSION),
    ];

    // Send request.
    let response = http.post_json(ANTHROPIC_URL, &headers, &body).await?;

    if response.status != 200 {
        return Err(AskError::UnexpectedResponse(format!(
            "API returned status {}: {}",
            response.status, response.body
        )));
    }

    extract_response_text(&response.body)
}

// ── Internal helpers ──────────────────────────────────────────────────────────

/// Split the first whitespace-delimited token from a string.
/// Returns `(token, rest)`.
fn split_first_token(s: &str) -> (&str, &str) {
    let s = s.trim_start();
    if let Some(pos) = s.find(char::is_whitespace) {
        (&s[..pos], &s[pos..])
    } else {
        (s, "")
    }
}

/// Strip surrounding double or single quotes from a string.
fn strip_quotes(s: &str) -> &str {
    let s = s.trim();
    if (s.starts_with('"') && s.ends_with('"'))
        || (s.starts_with('\'') && s.ends_with('\''))
    {
        &s[1..s.len() - 1]
    } else {
        s
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── AskInvocation::parse ──────────────────────────────────────────────────

    #[test]
    fn parse_plain_prompt() {
        let inv = AskInvocation::parse("ask what is 2+2").unwrap();
        assert_eq!(inv.prompt, "what is 2+2");
        assert!(!inv.fresh);
        assert!(inv.model_override.is_none());
    }

    #[test]
    fn parse_quoted_prompt() {
        let inv = AskInvocation::parse(r#"ask "what is 2+2""#).unwrap();
        assert_eq!(inv.prompt, "what is 2+2");
    }

    #[test]
    fn parse_fresh_flag() {
        let inv = AskInvocation::parse("ask --fresh what happened").unwrap();
        assert!(inv.fresh);
        assert_eq!(inv.prompt, "what happened");
    }

    #[test]
    fn parse_no_transcript_alias() {
        let inv = AskInvocation::parse("ask --no-transcript hello").unwrap();
        assert!(inv.fresh);
    }

    #[test]
    fn parse_model_override() {
        let inv =
            AskInvocation::parse("ask --model anthropic/claude-3-5-sonnet what")
                .unwrap();
        assert_eq!(
            inv.model_override.as_deref(),
            Some("anthropic/claude-3-5-sonnet")
        );
        assert_eq!(inv.prompt, "what");
    }

    #[test]
    fn parse_fresh_and_model() {
        let inv =
            AskInvocation::parse("ask --fresh --model gpt-4o explain this").unwrap();
        assert!(inv.fresh);
        assert_eq!(inv.model_override.as_deref(), Some("gpt-4o"));
        assert_eq!(inv.prompt, "explain this");
    }

    #[test]
    fn parse_empty_prompt_errors() {
        assert!(AskInvocation::parse("ask").is_err());
        assert!(AskInvocation::parse("ask --fresh").is_err());
    }

    // ── Request construction ──────────────────────────────────────────────────

    #[test]
    fn build_user_content_with_transcript() {
        let content = build_user_content("[input] echo hi\n[output] hi", "what did I run?");
        assert!(content.contains("[input] echo hi"));
        assert!(content.contains("what did I run?"));
    }

    #[test]
    fn build_user_content_fresh() {
        let content = build_user_content("", "just the prompt");
        assert_eq!(content, "just the prompt");
    }

    #[test]
    fn strip_provider_prefix_removes_prefix() {
        assert_eq!(
            strip_provider_prefix("anthropic/claude-sonnet-4-5"),
            "claude-sonnet-4-5"
        );
        assert_eq!(strip_provider_prefix("claude-sonnet-4-5"), "claude-sonnet-4-5");
    }

    #[test]
    fn build_request_body_is_valid_json() {
        let body = build_request_body("claude-sonnet-4-5", "hello");
        let v: Value = serde_json::from_str(&body).expect("should be valid JSON");
        assert_eq!(v["model"], "claude-sonnet-4-5");
        assert_eq!(v["messages"][0]["role"], "user");
        assert_eq!(v["messages"][0]["content"], "hello");
    }

    // ── Response parsing ──────────────────────────────────────────────────────

    #[test]
    fn extract_response_text_happy_path() {
        let body = r#"{
            "content": [{"type": "text", "text": "The answer is 42."}],
            "stop_reason": "end_turn"
        }"#;
        let text = extract_response_text(body).unwrap();
        assert_eq!(text, "The answer is 42.");
    }

    #[test]
    fn extract_response_text_api_error() {
        let body = r#"{"error": {"type": "auth", "message": "invalid api key"}}"#;
        let err = extract_response_text(body).unwrap_err();
        assert!(err.to_string().contains("invalid api key"));
    }

    #[test]
    fn extract_response_text_malformed() {
        let err = extract_response_text("not json").unwrap_err();
        assert!(err.to_string().contains("invalid JSON"));
    }
}
