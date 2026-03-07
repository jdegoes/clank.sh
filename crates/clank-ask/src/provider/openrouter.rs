use std::sync::Arc;

use async_trait::async_trait;
use clank_http::{HttpClient, HttpError, Request};
use serde_json::json;

use super::wire::{ChatMessage, ChatRequest, ChatResponse};
use super::{CompletionRequest, CompletionResponse, ModelProvider, ProviderError};

/// Model provider implementation for OpenRouter.
///
/// OpenRouter exposes an OpenAI-compatible chat completions API at
/// `https://openrouter.ai/api/v1/chat/completions`. It accepts any
/// `provider/model-name` string and routes to the appropriate upstream
/// provider, giving clank access to hundreds of models through a single
/// API key.
pub struct OpenRouterProvider {
    api_key: String,
    http: Arc<dyn HttpClient>,
    /// Base URL — overridable in tests.
    base_url: String,
}

impl OpenRouterProvider {
    pub fn new(api_key: impl Into<String>, http: Arc<dyn HttpClient>) -> Self {
        Self {
            api_key: api_key.into(),
            http,
            base_url: "https://openrouter.ai/api/v1".to_string(),
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
// Implementation
// ---------------------------------------------------------------------------

#[async_trait]
impl ModelProvider for OpenRouterProvider {
    async fn complete(
        &self,
        request: CompletionRequest,
    ) -> Result<CompletionResponse, ProviderError> {
        // OpenAI wire format: system prompt is the first message with role
        // "system", not a top-level field. See:
        // https://platform.openai.com/docs/api-reference/chat/create
        let mut messages: Vec<ChatMessage<'_>> = Vec::new();
        if !request.system_prompt.is_empty() {
            messages.push(ChatMessage {
                role: "system",
                content: &request.system_prompt,
            });
        }
        messages.extend(request.messages.iter().map(|m| ChatMessage {
            role: match m.role {
                super::Role::User => "user",
                super::Role::Assistant => "assistant",
            },
            content: &m.content,
        }));

        let body = json!(ChatRequest {
            // Pass the full model string through — OpenRouter uses
            // provider/model-name notation natively.
            model: &request.model,
            max_tokens: 4096,
            messages,
            stream: false,
        });

        let body_bytes = serde_json::to_vec(&body)
            .map_err(|e| ProviderError::Other(format!("failed to serialise request: {e}")))?;

        let req = Request {
            method: "POST".to_string(),
            url: format!("{}/chat/completions", self.base_url),
            headers: vec![
                (
                    "authorization".to_string(),
                    format!("Bearer {}", self.api_key),
                ),
                ("content-type".to_string(), "application/json".to_string()),
                ("http-referer".to_string(), "https://clank.sh".to_string()),
                ("x-openrouter-title".to_string(), "clank.sh".to_string()),
            ],
            body: Some(body_bytes),
        };

        let resp = self.http.send(req).await.map_err(|e| match e {
            HttpError::Timeout => ProviderError::Timeout,
            HttpError::ConnectionFailed(msg) => ProviderError::RemoteCallFailed(msg),
            HttpError::NonSuccessResponse { status, body } => match status {
                401 | 402 => {
                    ProviderError::NotConfigured(format!("OpenRouter returned {status}: {body}"))
                }
                429 => ProviderError::Timeout,
                _ => ProviderError::RemoteCallFailed(format!("HTTP {status}: {body}")),
            },
            HttpError::Tls(msg) => ProviderError::RemoteCallFailed(msg),
        })?;

        let api_resp: ChatResponse = serde_json::from_slice(&resp.body).map_err(|e| {
            ProviderError::Other(format!(
                "failed to parse OpenRouter response: {e}\nraw: {}",
                String::from_utf8_lossy(&resp.body)
            ))
        })?;

        let text = api_resp
            .choices
            .into_iter()
            .next()
            .map(|c| c.message.content)
            .unwrap_or_default();

        Ok(CompletionResponse { content: text })
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use clank_http::{MockHttpClient, MockResponse};

    use super::super::{Message, Role};
    use super::*;

    fn make_provider(responses: Vec<MockResponse>) -> OpenRouterProvider {
        OpenRouterProvider::new("sk-or-test", Arc::new(MockHttpClient::new(responses)))
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

    fn success_response(text: &str) -> MockResponse {
        let body = serde_json::json!({
            "id": "gen-test",
            "choices": [{"message": {"role": "assistant", "content": text}}],
            "model": "anthropic/claude-sonnet-4-5",
            "usage": {"prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15}
        });
        MockResponse::json(body.to_string())
    }

    #[tokio::test]
    async fn test_openrouter_mock_success() {
        let provider = make_provider(vec![success_response("Hello from OpenRouter!")]);
        let resp = provider.complete(make_request()).await.unwrap();
        assert_eq!(resp.content, "Hello from OpenRouter!");
    }

    #[tokio::test]
    async fn test_openrouter_builds_correct_request() {
        let mock = Arc::new(MockHttpClient::new(vec![success_response("ok")]));
        let provider =
            OpenRouterProvider::new("sk-or-test", Arc::clone(&mock) as Arc<dyn HttpClient>);
        provider.complete(make_request()).await.unwrap();

        let reqs = mock.requests.lock().await;
        assert_eq!(reqs.len(), 1);
        let req = &reqs[0];

        // Correct endpoint
        assert!(req.url.contains("/chat/completions"));

        // Authorization header uses Bearer scheme
        assert!(req
            .headers
            .iter()
            .any(|(k, v)| k == "authorization" && v.starts_with("Bearer ")));

        // Full model string passed through — no prefix stripping
        let body: serde_json::Value = serde_json::from_slice(req.body.as_deref().unwrap()).unwrap();
        assert_eq!(
            body["model"], "anthropic/claude-sonnet-4-5",
            "model string must not be stripped for OpenRouter"
        );

        // OpenAI wire format spec: system prompt must be messages[0] with
        // role "system", NOT a top-level "system" field (which is an
        // Anthropic-specific extension and is silently ignored by OpenRouter).
        // https://platform.openai.com/docs/api-reference/chat/create
        assert!(
            body.get("system").is_none(),
            "top-level 'system' field must not be present in OpenAI wire format"
        );
        let messages = body["messages"].as_array().unwrap();
        assert_eq!(
            messages[0]["role"], "system",
            "first message must have role 'system'"
        );
        assert_eq!(
            messages[0]["content"], "You are a test assistant.",
            "first message content must be the system prompt"
        );
        assert_eq!(
            messages[1]["role"], "user",
            "second message must be the user turn"
        );

        // App identification headers present
        assert!(req.headers.iter().any(|(k, _)| k == "http-referer"));
        assert!(req.headers.iter().any(|(k, _)| k == "x-openrouter-title"));
    }

    #[tokio::test]
    async fn test_openrouter_mock_timeout() {
        let mock = Arc::new(MockHttpClient::with_results(vec![Err(HttpError::Timeout)]));
        let provider = OpenRouterProvider::new("key", Arc::clone(&mock) as Arc<dyn HttpClient>);
        let err = provider.complete(make_request()).await.unwrap_err();
        assert!(matches!(err, ProviderError::Timeout));
        assert_eq!(err.exit_code(), 3);
    }

    #[tokio::test]
    async fn test_openrouter_rate_limit_maps_to_timeout() {
        // 429 Too Many Requests should map to Timeout (exit 3)
        let mock = Arc::new(MockHttpClient::with_results(vec![Err(
            HttpError::NonSuccessResponse {
                status: 429,
                body: "rate limited".to_string(),
            },
        )]));
        let provider = OpenRouterProvider::new("key", Arc::clone(&mock) as Arc<dyn HttpClient>);
        let err = provider.complete(make_request()).await.unwrap_err();
        assert!(matches!(err, ProviderError::Timeout));
        assert_eq!(err.exit_code(), 3);
    }

    #[tokio::test]
    async fn test_openrouter_mock_http_error() {
        let mock = Arc::new(MockHttpClient::with_results(vec![Err(
            HttpError::NonSuccessResponse {
                status: 502,
                body: "Bad Gateway".to_string(),
            },
        )]));
        let provider = OpenRouterProvider::new("key", Arc::clone(&mock) as Arc<dyn HttpClient>);
        let err = provider.complete(make_request()).await.unwrap_err();
        assert!(matches!(err, ProviderError::RemoteCallFailed(_)));
        assert_eq!(err.exit_code(), 4);
    }

    #[tokio::test]
    async fn test_openrouter_mock_unauthorized() {
        let mock = Arc::new(MockHttpClient::with_results(vec![Err(
            HttpError::NonSuccessResponse {
                status: 401,
                body: "Unauthorized".to_string(),
            },
        )]));
        let provider = OpenRouterProvider::new("bad-key", Arc::clone(&mock) as Arc<dyn HttpClient>);
        let err = provider.complete(make_request()).await.unwrap_err();
        assert!(matches!(err, ProviderError::NotConfigured(_)));
        assert_eq!(err.exit_code(), 1);
    }
}
