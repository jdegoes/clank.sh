use std::sync::Arc;

use async_trait::async_trait;
use clank_http::{HttpClient, HttpError, Request};
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::{CompletionRequest, CompletionResponse, ModelProvider, ProviderError, Role};

/// Model provider implementation for Ollama's native `/api/chat` endpoint.
///
/// Ollama runs locally and requires no API key. The model string convention
/// is `ollama/<name>` in clank; the `"ollama/"` prefix is stripped before
/// the request is sent to Ollama, which expects just `"<name>"`.
pub struct OllamaProvider {
    base_url: String,
    http: Arc<dyn HttpClient>,
}

impl OllamaProvider {
    /// Create a new Ollama provider.
    ///
    /// `base_url` must be a resolved, concrete URL string. Default resolution
    /// (`"http://localhost:11434"`) is the caller's responsibility and happens
    /// in `select_provider`.
    pub fn new(base_url: impl Into<String>, http: Arc<dyn HttpClient>) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            http,
        }
    }
}

// ---------------------------------------------------------------------------
// Ollama wire types (private to this module)
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct OllamaChatRequest {
    model: String,
    messages: Vec<OllamaMessage>,
    stream: bool,
}

#[derive(Serialize, Deserialize)]
struct OllamaMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct OllamaChatResponse {
    message: OllamaMessage,
}

// ---------------------------------------------------------------------------
// Implementation
// ---------------------------------------------------------------------------

#[async_trait]
impl ModelProvider for OllamaProvider {
    async fn complete(
        &self,
        request: CompletionRequest,
    ) -> Result<CompletionResponse, ProviderError> {
        // Strip the "ollama/" provider prefix — Ollama expects just the model name.
        let model = request
            .model
            .strip_prefix("ollama/")
            .unwrap_or(&request.model)
            .to_string();

        // Build the messages array. Ollama's /api/chat endpoint supports a
        // system role message, so we prepend the system prompt as a message
        // rather than a top-level field.
        let mut messages = Vec::with_capacity(request.messages.len() + 1);
        if !request.system_prompt.is_empty() {
            messages.push(OllamaMessage {
                role: "system".to_string(),
                content: request.system_prompt.clone(),
            });
        }
        for m in &request.messages {
            messages.push(OllamaMessage {
                role: match m.role {
                    Role::User => "user".to_string(),
                    Role::Assistant => "assistant".to_string(),
                },
                content: m.content.clone(),
            });
        }

        let body = json!(OllamaChatRequest {
            model: model.clone(),
            messages,
            stream: false,
        });

        let body_bytes = serde_json::to_vec(&body)
            .map_err(|e| ProviderError::Other(format!("failed to serialise request: {e}")))?;

        let req = Request {
            method: "POST".to_string(),
            url: format!("{}/api/chat", self.base_url),
            headers: vec![("content-type".to_string(), "application/json".to_string())],
            body: Some(body_bytes),
        };

        let resp = self.http.send(req).await.map_err(|e| match e {
            HttpError::Timeout => ProviderError::Timeout,
            HttpError::ConnectionFailed(_) => ProviderError::RemoteCallFailed(format!(
                "Ollama is not running at {}. Start it with: ollama serve",
                self.base_url
            )),
            HttpError::NonSuccessResponse { status: 404, .. } => ProviderError::RemoteCallFailed(
                format!("Model '{model}' not found. Pull it with: ollama pull {model}"),
            ),
            HttpError::NonSuccessResponse { status, body } => {
                ProviderError::RemoteCallFailed(format!("Ollama returned {status}: {body}"))
            }
            HttpError::Tls(msg) => ProviderError::RemoteCallFailed(msg),
        })?;

        let api_resp: OllamaChatResponse = serde_json::from_slice(&resp.body).map_err(|e| {
            ProviderError::Other(format!(
                "failed to parse Ollama response: {e}\nraw: {}",
                String::from_utf8_lossy(&resp.body)
            ))
        })?;

        Ok(CompletionResponse {
            content: api_resp.message.content,
        })
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

    fn make_provider(responses: Vec<MockResponse>) -> OllamaProvider {
        OllamaProvider::new(
            "http://localhost:11434",
            Arc::new(MockHttpClient::new(responses)),
        )
    }

    fn make_request() -> CompletionRequest {
        CompletionRequest {
            model: "ollama/llama3.2".to_string(),
            system_prompt: "You are a test assistant.".to_string(),
            messages: vec![Message {
                role: Role::User,
                content: "Hello".to_string(),
            }],
        }
    }

    fn success_response(text: &str) -> MockResponse {
        let body = serde_json::json!({
            "model": "llama3.2",
            "message": { "role": "assistant", "content": text },
            "done": true
        });
        MockResponse::json(body.to_string())
    }

    #[tokio::test]
    async fn test_ollama_builds_correct_request() {
        let mock = Arc::new(MockHttpClient::new(vec![success_response("ok")]));
        let provider = OllamaProvider::new(
            "http://localhost:11434",
            Arc::clone(&mock) as Arc<dyn HttpClient>,
        );
        provider.complete(make_request()).await.unwrap();

        let reqs = mock.requests.lock().await;
        assert_eq!(reqs.len(), 1);
        let req = &reqs[0];

        assert_eq!(req.method, "POST");
        assert!(req.url.ends_with("/api/chat"), "url was: {}", req.url);

        let body: serde_json::Value = serde_json::from_slice(req.body.as_deref().unwrap()).unwrap();

        // Provider prefix must be stripped.
        assert_eq!(body["model"], "llama3.2");
        assert_eq!(body["stream"], false);

        // System prompt is first message, user prompt is second.
        let messages = body["messages"].as_array().unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0]["role"], "system");
        assert_eq!(messages[0]["content"], "You are a test assistant.");
        assert_eq!(messages[1]["role"], "user");
        assert_eq!(messages[1]["content"], "Hello");
    }

    #[tokio::test]
    async fn test_ollama_mock_success() {
        let provider = make_provider(vec![success_response("Hello from Ollama!")]);
        let resp = provider.complete(make_request()).await.unwrap();
        assert_eq!(resp.content, "Hello from Ollama!");
    }

    #[tokio::test]
    async fn test_ollama_mock_404() {
        let mock = Arc::new(MockHttpClient::with_results(vec![Err(
            HttpError::NonSuccessResponse {
                status: 404,
                body: "model 'llama3.2' not found".to_string(),
            },
        )]));
        let provider = OllamaProvider::new(
            "http://localhost:11434",
            Arc::clone(&mock) as Arc<dyn HttpClient>,
        );
        let err = provider.complete(make_request()).await.unwrap_err();
        assert!(matches!(err, ProviderError::RemoteCallFailed(_)));
        let msg = err.to_string();
        assert!(msg.contains("not found"), "expected 'not found' in: {msg}");
        assert!(msg.contains("ollama pull"), "expected pull hint in: {msg}");
        assert_eq!(err.exit_code(), 4);
    }

    #[tokio::test]
    async fn test_ollama_mock_connection_refused() {
        let mock = Arc::new(MockHttpClient::with_results(vec![Err(
            HttpError::ConnectionFailed("connection refused".to_string()),
        )]));
        let provider = OllamaProvider::new(
            "http://localhost:11434",
            Arc::clone(&mock) as Arc<dyn HttpClient>,
        );
        let err = provider.complete(make_request()).await.unwrap_err();
        assert!(matches!(err, ProviderError::RemoteCallFailed(_)));
        let msg = err.to_string();
        assert!(
            msg.contains("not running"),
            "expected 'not running' in: {msg}"
        );
        assert!(
            msg.contains("ollama serve"),
            "expected serve hint in: {msg}"
        );
        assert_eq!(err.exit_code(), 4);
    }

    #[tokio::test]
    async fn test_ollama_mock_timeout() {
        let mock = Arc::new(MockHttpClient::with_results(vec![Err(HttpError::Timeout)]));
        let provider = OllamaProvider::new(
            "http://localhost:11434",
            Arc::clone(&mock) as Arc<dyn HttpClient>,
        );
        let err = provider.complete(make_request()).await.unwrap_err();
        assert!(matches!(err, ProviderError::Timeout));
        assert_eq!(err.exit_code(), 3);
    }

    #[tokio::test]
    async fn test_ollama_custom_base_url_used_in_request() {
        let mock = Arc::new(MockHttpClient::new(vec![success_response("ok")]));
        let provider = OllamaProvider::new(
            "http://myhost:11434",
            Arc::clone(&mock) as Arc<dyn HttpClient>,
        );
        provider.complete(make_request()).await.unwrap();

        let reqs = mock.requests.lock().await;
        assert!(
            reqs[0].url.starts_with("http://myhost:11434"),
            "expected custom base_url in request, got: {}",
            reqs[0].url
        );
    }

    #[tokio::test]
    async fn test_ollama_trailing_slash_in_base_url_is_normalized() {
        // A trailing slash in base_url must not produce a double-slash URL.
        let mock = Arc::new(MockHttpClient::new(vec![success_response("ok")]));
        let provider = OllamaProvider::new(
            "http://localhost:11434/",
            Arc::clone(&mock) as Arc<dyn HttpClient>,
        );
        provider.complete(make_request()).await.unwrap();

        let reqs = mock.requests.lock().await;
        assert!(
            !reqs[0].url.contains("//api/chat"),
            "double-slash must not appear in URL: {}",
            reqs[0].url
        );
        assert!(
            reqs[0].url.ends_with("/api/chat"),
            "URL must end with /api/chat: {}",
            reqs[0].url
        );
    }
}
