//! Ollama provider implementation.
//!
//! Calls `POST {base_url}/api/chat` with `stream: false` and parses the
//! response at `.message.content`.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use clank_http::HttpClient;

use crate::{Message, ProviderError, Role};

pub struct OllamaProvider<H> {
    http: Arc<H>,
    base_url: String,
    model: String,
}

impl<H: HttpClient> OllamaProvider<H> {
    pub fn new(http: Arc<H>, base_url: String, model: String) -> Self {
        Self {
            http,
            base_url,
            model,
        }
    }

    pub async fn complete(&self, messages: &[Message]) -> Result<String, ProviderError> {
        let request = OllamaChatRequest {
            model: &self.model,
            messages: messages.iter().map(OllamaMessage::from).collect(),
            stream: false,
        };

        let body = serde_json::to_vec(&request).map_err(|e| ProviderError::Parse(e.to_string()))?;

        let url = format!("{}/api/chat", self.base_url);
        let headers = [("Content-Type", "application/json")];

        let response = self
            .http
            .post(&url, &headers, &body)
            .await
            .map_err(|e| match e {
                clank_http::HttpError::Transport(msg) => ProviderError::Transport(msg),
                clank_http::HttpError::Status(code) => ProviderError::Status(code),
            })?;

        let text = response
            .text()
            .map_err(|e: std::str::Utf8Error| ProviderError::Parse(e.to_string()))?;

        let parsed: OllamaChatResponse =
            serde_json::from_str(text).map_err(|e| ProviderError::Parse(e.to_string()))?;

        if parsed.message.content.is_empty() {
            return Err(ProviderError::Parse("empty response from Ollama".into()));
        }

        Ok(parsed.message.content)
    }
}

// ---------------------------------------------------------------------------
// Serde types
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct OllamaChatRequest<'a> {
    model: &'a str,
    messages: Vec<OllamaMessage>,
    stream: bool,
}

#[derive(Serialize)]
struct OllamaMessage {
    role: &'static str,
    content: String,
}

impl From<&Message> for OllamaMessage {
    fn from(m: &Message) -> Self {
        OllamaMessage {
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
struct OllamaChatResponse {
    message: OllamaResponseMessage,
}

#[derive(Deserialize)]
struct OllamaResponseMessage {
    content: String,
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
        let messages: Vec<OllamaMessage> =
            make_messages().iter().map(OllamaMessage::from).collect();
        let request = OllamaChatRequest {
            model: "llama3.2",
            messages,
            stream: false,
        };
        let json = serde_json::to_value(&request).unwrap();
        assert_eq!(json["model"], "llama3.2");
        assert_eq!(json["stream"], false);
        assert_eq!(json["messages"][0]["role"], "system");
        assert_eq!(json["messages"][1]["role"], "user");
    }

    #[test]
    fn parses_successful_response() {
        let raw = r#"{
            "model": "llama3.2",
            "created_at": "2023-12-12T14:13:43.416799Z",
            "message": { "role": "assistant", "content": "Hello there!" },
            "done": true
        }"#;
        let parsed: OllamaChatResponse = serde_json::from_str(raw).unwrap();
        assert_eq!(parsed.message.content, "Hello there!");
    }

    #[test]
    fn empty_content_is_detected() {
        let raw = r#"{
            "model": "llama3.2",
            "created_at": "2023-12-12T14:13:43.416799Z",
            "message": { "role": "assistant", "content": "" },
            "done": true
        }"#;
        let parsed: OllamaChatResponse = serde_json::from_str(raw).unwrap();
        assert!(parsed.message.content.is_empty());
    }

    #[test]
    fn malformed_response_fails_to_parse() {
        let raw = r#"{ "not_message": {} }"#;
        let result = serde_json::from_str::<OllamaChatResponse>(raw);
        assert!(result.is_err());
    }
}
