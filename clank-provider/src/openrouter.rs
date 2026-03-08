//! OpenRouter provider implementation.
//!
//! Calls `POST https://openrouter.ai/api/v1/chat/completions` and parses the
//! OpenAI-compatible response at `.choices[0].message.content`.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use clank_http::HttpClient;

use crate::{Message, ProviderError, Role};

const OPENROUTER_URL: &str = "https://openrouter.ai/api/v1/chat/completions";

pub struct OpenRouterProvider<H> {
    http: Arc<H>,
    model: String,
    /// The API key is stored but must never appear in logs or error messages.
    api_key: String,
}

impl<H: HttpClient> OpenRouterProvider<H> {
    pub fn new(http: Arc<H>, model: String, api_key: String) -> Self {
        Self {
            http,
            model,
            api_key,
        }
    }

    pub async fn complete(&self, messages: &[Message]) -> Result<String, ProviderError> {
        let request = OpenRouterRequest {
            model: &self.model,
            messages: messages.iter().map(OpenRouterMessage::from).collect(),
            stream: false,
        };

        let body = serde_json::to_vec(&request).map_err(|e| ProviderError::Parse(e.to_string()))?;

        let auth_header = format!("Bearer {}", self.api_key);
        let headers = [
            ("Content-Type", "application/json"),
            ("Authorization", auth_header.as_str()),
            ("HTTP-Referer", "https://clank.sh"),
            ("X-OpenRouter-Title", "clank.sh"),
        ];

        let response = self
            .http
            .post(OPENROUTER_URL, &headers, &body)
            .await
            .map_err(|e| match e {
                clank_http::HttpError::Transport(msg) => ProviderError::Transport(msg),
                clank_http::HttpError::Status(code) => ProviderError::Status(code),
            })?;

        let text = response
            .text()
            .map_err(|e: std::str::Utf8Error| ProviderError::Parse(e.to_string()))?;

        let parsed: OpenRouterResponse =
            serde_json::from_str(text).map_err(|e| ProviderError::Parse(e.to_string()))?;

        let content = parsed
            .choices
            .into_iter()
            .next()
            .and_then(|c| c.message.content)
            .ok_or_else(|| ProviderError::Parse("no content in OpenRouter response".into()))?;

        if content.is_empty() {
            return Err(ProviderError::Parse(
                "empty response from OpenRouter".into(),
            ));
        }

        Ok(content)
    }
}

// ---------------------------------------------------------------------------
// Serde types
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct OpenRouterRequest<'a> {
    model: &'a str,
    messages: Vec<OpenRouterMessage>,
    stream: bool,
}

#[derive(Serialize)]
struct OpenRouterMessage {
    role: &'static str,
    content: String,
}

impl From<&Message> for OpenRouterMessage {
    fn from(m: &Message) -> Self {
        OpenRouterMessage {
            role: match m.role {
                Role::System => "system",
                Role::User => "user",
                Role::Assistant => "assistant",
            },
            content: m.content.clone(),
        }
    }
}

#[derive(Deserialize)]
struct OpenRouterResponse {
    choices: Vec<OpenRouterChoice>,
}

#[derive(Deserialize)]
struct OpenRouterChoice {
    message: OpenRouterChoiceMessage,
}

#[derive(Deserialize)]
struct OpenRouterChoiceMessage {
    content: Option<String>,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Role;

    fn make_messages() -> Vec<Message> {
        vec![
            Message {
                role: Role::System,
                content: "You are a helpful assistant.".into(),
            },
            Message {
                role: Role::User,
                content: "Say hello.".into(),
            },
        ]
    }

    #[test]
    fn request_serializes_correctly() {
        let messages: Vec<OpenRouterMessage> = make_messages()
            .iter()
            .map(OpenRouterMessage::from)
            .collect();
        let request = OpenRouterRequest {
            model: "anthropic/claude-3-5-haiku",
            messages,
            stream: false,
        };
        let json = serde_json::to_value(&request).unwrap();
        assert_eq!(json["model"], "anthropic/claude-3-5-haiku");
        assert_eq!(json["stream"], false);
        assert_eq!(json["messages"][0]["role"], "system");
        assert_eq!(json["messages"][1]["role"], "user");
    }

    #[test]
    fn parses_successful_response() {
        let raw = r#"{
            "id": "gen-123",
            "choices": [{
                "finish_reason": "stop",
                "message": { "role": "assistant", "content": "Hello there!" }
            }],
            "model": "anthropic/claude-3-5-haiku"
        }"#;
        let parsed: OpenRouterResponse = serde_json::from_str(raw).unwrap();
        let content = parsed
            .choices
            .into_iter()
            .next()
            .and_then(|c| c.message.content)
            .unwrap();
        assert_eq!(content, "Hello there!");
    }

    #[test]
    fn null_content_returns_none() {
        let raw = r#"{
            "id": "gen-123",
            "choices": [{
                "finish_reason": "stop",
                "message": { "role": "assistant", "content": null }
            }],
            "model": "anthropic/claude-3-5-haiku"
        }"#;
        let parsed: OpenRouterResponse = serde_json::from_str(raw).unwrap();
        let content = parsed
            .choices
            .into_iter()
            .next()
            .and_then(|c| c.message.content);
        assert!(content.is_none());
    }

    #[test]
    fn empty_choices_array() {
        let raw = r#"{ "id": "gen-123", "choices": [], "model": "x" }"#;
        let parsed: OpenRouterResponse = serde_json::from_str(raw).unwrap();
        assert!(parsed.choices.is_empty());
    }

    #[test]
    fn malformed_response_fails_to_parse() {
        let raw = r#"{ "not_choices": [] }"#;
        let result = serde_json::from_str::<OpenRouterResponse>(raw);
        assert!(result.is_err());
    }
}
