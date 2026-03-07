use std::sync::Arc;

use async_trait::async_trait;
use clank_http::{HttpClient, HttpError, Request};
use serde_json::json;

use super::wire::{ChatMessage, ChatRequest, ChatResponse};
use super::{CompletionRequest, CompletionResponse, ModelProvider, ProviderError, Role};

/// Model provider for any server exposing the OpenAI `/v1/chat/completions`
/// API format.
///
/// Covers llama.cpp (`llama-server`), LM Studio, vLLM, LocalAI, and any
/// future OpenAI-compatible server. The base URL is required and has no
/// default — different tools use different ports. The API key is optional:
/// the `Authorization: Bearer` header is only included when `api_key` is
/// `Some(s)` where `s` is non-empty.
///
/// Model string convention: `openai-compat/<name>` in clank. The prefix is
/// stripped and `<name>` is sent as the `model` field. Single-model servers
/// (e.g. llama-server) ignore this field; multi-model servers use it.
pub struct OpenAiCompatProvider {
    base_url: String,
    api_key: Option<String>,
    http: Arc<dyn HttpClient>,
}

impl OpenAiCompatProvider {
    /// Create a new OpenAI-compatible provider.
    ///
    /// `base_url` is required and must be a concrete URL string (e.g.
    /// `"http://localhost:8080"`). `api_key` is optional; if `None` or
    /// `Some("")` no `Authorization` header is sent.
    pub fn new(
        base_url: impl Into<String>,
        api_key: Option<String>,
        http: Arc<dyn HttpClient>,
    ) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            api_key,
            http,
        }
    }
}

// ---------------------------------------------------------------------------
// Implementation
// ---------------------------------------------------------------------------

#[async_trait]
impl ModelProvider for OpenAiCompatProvider {
    async fn complete(
        &self,
        request: CompletionRequest,
    ) -> Result<CompletionResponse, ProviderError> {
        // Strip the "openai-compat/" provider prefix.
        let model = request
            .model
            .strip_prefix("openai-compat/")
            .unwrap_or(&request.model);

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
                Role::User => "user",
                Role::Assistant => "assistant",
            },
            content: &m.content,
        }));

        let body = json!(ChatRequest {
            model,
            max_tokens: 4096,
            messages,
            stream: false,
        });

        let body_bytes = serde_json::to_vec(&body)
            .map_err(|e| ProviderError::Other(format!("failed to serialise request: {e}")))?;

        // Include Authorization header only when api_key is non-empty.
        let mut headers = vec![("content-type".to_string(), "application/json".to_string())];
        if let Some(key) = &self.api_key {
            if !key.is_empty() {
                headers.push(("authorization".to_string(), format!("Bearer {key}")));
            }
        }

        let req = Request {
            method: "POST".to_string(),
            url: format!("{}/v1/chat/completions", self.base_url),
            headers,
            body: Some(body_bytes),
        };

        let resp = self.http.send(req).await.map_err(|e| match e {
            HttpError::Timeout => ProviderError::Timeout,
            HttpError::ConnectionFailed(msg) => ProviderError::RemoteCallFailed(msg),
            HttpError::NonSuccessResponse { status, body } => match status {
                401 | 402 => {
                    ProviderError::NotConfigured(format!("server returned {status}: {body}"))
                }
                _ => ProviderError::RemoteCallFailed(format!("HTTP {status}: {body}")),
            },
            HttpError::Tls(msg) => ProviderError::RemoteCallFailed(msg),
        })?;

        let api_resp: ChatResponse = serde_json::from_slice(&resp.body).map_err(|e| {
            ProviderError::Other(format!(
                "failed to parse response: {e}\nraw: {}",
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

    fn make_provider_no_key(responses: Vec<MockResponse>) -> OpenAiCompatProvider {
        OpenAiCompatProvider::new(
            "http://localhost:8080",
            None,
            Arc::new(MockHttpClient::new(responses)),
        )
    }

    fn make_request() -> CompletionRequest {
        CompletionRequest {
            model: "openai-compat/phi4".to_string(),
            system_prompt: "You are a test assistant.".to_string(),
            messages: vec![Message {
                role: Role::User,
                content: "Hello".to_string(),
            }],
        }
    }

    fn success_response(text: &str) -> MockResponse {
        let body = serde_json::json!({
            "id": "cmpl-test",
            "choices": [{"message": {"role": "assistant", "content": text}}],
            "model": "phi4",
            "usage": {"prompt_tokens": 5, "completion_tokens": 5, "total_tokens": 10}
        });
        MockResponse::json(body.to_string())
    }

    #[tokio::test]
    async fn test_openai_compat_omits_auth_header_when_no_key() {
        let mock = Arc::new(MockHttpClient::new(vec![success_response("ok")]));
        let provider = OpenAiCompatProvider::new(
            "http://localhost:8080",
            None,
            Arc::clone(&mock) as Arc<dyn HttpClient>,
        );
        provider.complete(make_request()).await.unwrap();

        let reqs = mock.requests.lock().await;
        assert!(
            !reqs[0].headers.iter().any(|(k, _)| k == "authorization"),
            "Authorization header must be absent when api_key is None"
        );
    }

    #[tokio::test]
    async fn test_openai_compat_omits_auth_header_when_empty_key() {
        let mock = Arc::new(MockHttpClient::new(vec![success_response("ok")]));
        let provider = OpenAiCompatProvider::new(
            "http://localhost:8080",
            Some(String::new()),
            Arc::clone(&mock) as Arc<dyn HttpClient>,
        );
        provider.complete(make_request()).await.unwrap();

        let reqs = mock.requests.lock().await;
        assert!(
            !reqs[0].headers.iter().any(|(k, _)| k == "authorization"),
            "Authorization header must be absent when api_key is Some(\"\")"
        );
    }

    #[tokio::test]
    async fn test_openai_compat_includes_auth_header_when_key_set() {
        let mock = Arc::new(MockHttpClient::new(vec![success_response("ok")]));
        let provider = OpenAiCompatProvider::new(
            "http://localhost:8080",
            Some("sk-x".to_string()),
            Arc::clone(&mock) as Arc<dyn HttpClient>,
        );
        provider.complete(make_request()).await.unwrap();

        let reqs = mock.requests.lock().await;
        let auth = reqs[0]
            .headers
            .iter()
            .find(|(k, _)| k == "authorization")
            .map(|(_, v)| v.as_str());
        assert_eq!(auth, Some("Bearer sk-x"));
    }

    #[tokio::test]
    async fn test_openai_compat_strips_provider_prefix_from_model() {
        let mock = Arc::new(MockHttpClient::new(vec![success_response("ok")]));
        let provider = OpenAiCompatProvider::new(
            "http://localhost:8080",
            None,
            Arc::clone(&mock) as Arc<dyn HttpClient>,
        );
        provider.complete(make_request()).await.unwrap();

        let reqs = mock.requests.lock().await;
        let body: serde_json::Value =
            serde_json::from_slice(reqs[0].body.as_deref().unwrap()).unwrap();
        assert_eq!(body["model"], "phi4", "provider prefix must be stripped");
    }

    #[tokio::test]
    async fn test_openai_compat_system_prompt_is_first_message() {
        // OpenAI wire format spec: system prompt must be messages[0] with
        // role "system", NOT a top-level "system" field (which is an
        // Anthropic-specific extension silently ignored by OpenAI-compatible
        // servers). https://platform.openai.com/docs/api-reference/chat/create
        let mock = Arc::new(MockHttpClient::new(vec![success_response("ok")]));
        let provider = OpenAiCompatProvider::new(
            "http://localhost:8080",
            None,
            Arc::clone(&mock) as Arc<dyn HttpClient>,
        );
        provider.complete(make_request()).await.unwrap();

        let reqs = mock.requests.lock().await;
        let body: serde_json::Value =
            serde_json::from_slice(reqs[0].body.as_deref().unwrap()).unwrap();

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
    }

    #[tokio::test]
    async fn test_openai_compat_mock_success() {
        let provider = make_provider_no_key(vec![success_response("Hello from local model!")]);
        let resp = provider.complete(make_request()).await.unwrap();
        assert_eq!(resp.content, "Hello from local model!");
    }

    #[tokio::test]
    async fn test_openai_compat_mock_http_error() {
        let mock = Arc::new(MockHttpClient::with_results(vec![Err(
            HttpError::NonSuccessResponse {
                status: 500,
                body: "Internal Server Error".to_string(),
            },
        )]));
        let provider = OpenAiCompatProvider::new(
            "http://localhost:8080",
            None,
            Arc::clone(&mock) as Arc<dyn HttpClient>,
        );
        let err = provider.complete(make_request()).await.unwrap_err();
        assert!(matches!(err, ProviderError::RemoteCallFailed(_)));
        assert_eq!(err.exit_code(), 4);
    }

    #[tokio::test]
    async fn test_openai_compat_mock_timeout() {
        let mock = Arc::new(MockHttpClient::with_results(vec![Err(HttpError::Timeout)]));
        let provider = OpenAiCompatProvider::new(
            "http://localhost:8080",
            None,
            Arc::clone(&mock) as Arc<dyn HttpClient>,
        );
        let err = provider.complete(make_request()).await.unwrap_err();
        assert!(matches!(err, ProviderError::Timeout));
        assert_eq!(err.exit_code(), 3);
    }

    #[tokio::test]
    async fn test_openai_compat_omits_openrouter_headers() {
        let mock = Arc::new(MockHttpClient::new(vec![success_response("ok")]));
        let provider = OpenAiCompatProvider::new(
            "http://localhost:8080",
            None,
            Arc::clone(&mock) as Arc<dyn HttpClient>,
        );
        provider.complete(make_request()).await.unwrap();

        let reqs = mock.requests.lock().await;
        assert!(
            !reqs[0].headers.iter().any(|(k, _)| k == "http-referer"),
            "http-referer must not be sent to local servers"
        );
        assert!(
            !reqs[0]
                .headers
                .iter()
                .any(|(k, _)| k == "x-openrouter-title"),
            "x-openrouter-title must not be sent to local servers"
        );
    }

    #[tokio::test]
    async fn test_openai_compat_request_goes_to_correct_endpoint() {
        let mock = Arc::new(MockHttpClient::new(vec![success_response("ok")]));
        let provider = OpenAiCompatProvider::new(
            "http://localhost:8080",
            None,
            Arc::clone(&mock) as Arc<dyn HttpClient>,
        );
        provider.complete(make_request()).await.unwrap();

        let reqs = mock.requests.lock().await;
        assert!(
            reqs[0].url.ends_with("/v1/chat/completions"),
            "expected /v1/chat/completions endpoint, got: {}",
            reqs[0].url
        );
    }
}
