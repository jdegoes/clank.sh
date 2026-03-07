use std::sync::Arc;

use async_trait::async_trait;
use clank_http::{HttpClient, HttpError, Request};
use serde::{Deserialize, Serialize};
use serde_json::json;

#[cfg(test)]
use super::{CompletionRequest, CompletionResponse, Message, ModelProvider, ProviderError, Role};
#[cfg(not(test))]
use super::{CompletionRequest, CompletionResponse, ModelProvider, ProviderError, Role};

/// Model provider implementation for the Anthropic Messages API.
pub struct AnthropicProvider {
    api_key: String,
    http: Arc<dyn HttpClient>,
    /// Base URL for the API — overridable in tests.
    base_url: String,
}

impl AnthropicProvider {
    pub fn new(api_key: impl Into<String>, http: Arc<dyn HttpClient>) -> Self {
        Self {
            api_key: api_key.into(),
            http,
            base_url: "https://api.anthropic.com".to_string(),
        }
    }

    /// Create a provider pointing at a custom base URL (for tests).
    pub fn with_base_url(
        api_key: impl Into<String>,
        http: Arc<dyn HttpClient>,
        base_url: impl Into<String>,
    ) -> Self {
        Self {
            api_key: api_key.into(),
            http,
            base_url: base_url.into().trim_end_matches('/').to_string(),
        }
    }
}

// ---------------------------------------------------------------------------
// API wire types
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct ApiRequest<'a> {
    model: &'a str,
    max_tokens: u32,
    system: &'a str,
    messages: Vec<ApiMessage<'a>>,
}

#[derive(Serialize)]
struct ApiMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Deserialize)]
struct ApiResponse {
    content: Vec<ContentBlock>,
}

#[derive(Deserialize)]
struct ContentBlock {
    #[serde(rename = "type")]
    kind: String,
    text: Option<String>,
}

// ---------------------------------------------------------------------------
// Implementation
// ---------------------------------------------------------------------------

#[async_trait]
impl ModelProvider for AnthropicProvider {
    async fn complete(
        &self,
        request: CompletionRequest,
    ) -> Result<CompletionResponse, ProviderError> {
        // Strip provider prefix from model name if present.
        let model = request
            .model
            .strip_prefix("anthropic/")
            .unwrap_or(&request.model);

        let messages: Vec<ApiMessage<'_>> = request
            .messages
            .iter()
            .map(|m| ApiMessage {
                role: match m.role {
                    Role::User => "user",
                    Role::Assistant => "assistant",
                },
                content: &m.content,
            })
            .collect();

        let body = json!(ApiRequest {
            model,
            max_tokens: 4096,
            system: &request.system_prompt,
            messages,
        });

        let body_bytes = serde_json::to_vec(&body)
            .map_err(|e| ProviderError::Other(format!("failed to serialise request: {e}")))?;

        let req = Request {
            method: "POST".to_string(),
            url: format!("{}/v1/messages", self.base_url),
            headers: vec![
                ("content-type".to_string(), "application/json".to_string()),
                ("x-api-key".to_string(), self.api_key.clone()),
                ("anthropic-version".to_string(), "2023-06-01".to_string()),
            ],
            body: Some(body_bytes),
        };

        let resp = self.http.send(req).await.map_err(|e| match e {
            HttpError::Timeout => ProviderError::Timeout,
            HttpError::ConnectionFailed(msg) => ProviderError::RemoteCallFailed(msg),
            HttpError::NonSuccessResponse { status, body } => {
                ProviderError::RemoteCallFailed(format!("HTTP {status}: {body}"))
            }
            HttpError::Tls(msg) => ProviderError::RemoteCallFailed(msg),
        })?;

        let api_resp: ApiResponse = serde_json::from_slice(&resp.body).map_err(|e| {
            ProviderError::Other(format!(
                "failed to parse Anthropic response: {e}\nraw: {}",
                String::from_utf8_lossy(&resp.body)
            ))
        })?;

        let text = api_resp
            .content
            .into_iter()
            .filter(|b| b.kind == "text")
            .filter_map(|b| b.text)
            .collect::<Vec<_>>()
            .join("");

        Ok(CompletionResponse { content: text })
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use clank_http::{MockHttpClient, MockResponse};

    use super::*;

    fn make_provider(responses: Vec<MockResponse>) -> AnthropicProvider {
        AnthropicProvider::new("test-key", Arc::new(MockHttpClient::new(responses)))
    }

    fn make_request() -> CompletionRequest {
        CompletionRequest {
            model: "anthropic/claude-sonnet-4-5".to_string(),
            system_prompt: "You are a test assistant.".to_string(),
            messages: vec![Message {
                role: Role::User,
                content: "Hello".to_string(),
            }],
        }
    }

    #[tokio::test]
    async fn test_anthropic_mock_success() {
        let body = serde_json::json!({
            "content": [{ "type": "text", "text": "Hello from mock!" }],
            "id": "msg_01",
            "model": "claude-sonnet-4-5",
            "role": "assistant",
            "stop_reason": "end_turn",
            "type": "message",
            "usage": { "input_tokens": 10, "output_tokens": 5 }
        });
        let provider = make_provider(vec![MockResponse::json(body.to_string())]);
        let resp = provider.complete(make_request()).await.unwrap();
        assert_eq!(resp.content, "Hello from mock!");
    }

    #[tokio::test]
    async fn test_anthropic_builds_correct_request_json() {
        let body = serde_json::json!({
            "content": [{ "type": "text", "text": "ok" }],
            "id": "msg_01", "model": "m", "role": "assistant",
            "stop_reason": "end_turn", "type": "message",
            "usage": { "input_tokens": 1, "output_tokens": 1 }
        });
        let mock = Arc::new(MockHttpClient::new(vec![MockResponse::json(
            body.to_string(),
        )]));
        let provider =
            AnthropicProvider::new("sk-ant-test", Arc::clone(&mock) as Arc<dyn HttpClient>);
        provider.complete(make_request()).await.unwrap();

        let requests = mock.requests.lock().await;
        assert_eq!(requests.len(), 1);
        let req = &requests[0];
        assert_eq!(req.method, "POST");
        assert!(req.url.contains("/v1/messages"));
        assert!(req
            .headers
            .iter()
            .any(|(k, v)| k == "x-api-key" && v == "sk-ant-test"));

        let body: serde_json::Value = serde_json::from_slice(req.body.as_deref().unwrap()).unwrap();
        assert_eq!(body["model"], "claude-sonnet-4-5"); // prefix stripped
        assert!(body["messages"].as_array().unwrap().len() == 1);
    }

    #[tokio::test]
    async fn test_anthropic_mock_timeout() {
        let mock = Arc::new(MockHttpClient::with_results(vec![Err(HttpError::Timeout)]));
        let provider = AnthropicProvider::new("key", Arc::clone(&mock) as Arc<dyn HttpClient>);
        let err = provider.complete(make_request()).await.unwrap_err();
        assert!(matches!(err, ProviderError::Timeout));
        assert_eq!(err.exit_code(), 3);
    }

    #[tokio::test]
    async fn test_anthropic_mock_http_error() {
        let mock = Arc::new(MockHttpClient::with_results(vec![Err(
            HttpError::NonSuccessResponse {
                status: 401,
                body: "Unauthorized".to_string(),
            },
        )]));
        let provider = AnthropicProvider::new("bad-key", Arc::clone(&mock) as Arc<dyn HttpClient>);
        let err = provider.complete(make_request()).await.unwrap_err();
        assert!(matches!(err, ProviderError::RemoteCallFailed(_)));
        assert_eq!(err.exit_code(), 4);
    }

    #[tokio::test]
    async fn test_anthropic_parses_streaming_response() {
        // For Phase 1 we collect the full response body non-streaming.
        // This test validates multi-block content is concatenated correctly.
        let body = serde_json::json!({
            "content": [
                { "type": "text", "text": "Hello " },
                { "type": "text", "text": "world!" }
            ],
            "id": "msg_02", "model": "m", "role": "assistant",
            "stop_reason": "end_turn", "type": "message",
            "usage": { "input_tokens": 5, "output_tokens": 5 }
        });
        let provider = make_provider(vec![MockResponse::json(body.to_string())]);
        let resp = provider.complete(make_request()).await.unwrap();
        assert_eq!(resp.content, "Hello world!");
    }
}
