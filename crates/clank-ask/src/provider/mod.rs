use async_trait::async_trait;

pub mod anthropic;
pub mod ollama;
pub mod openai_compat;
pub mod openrouter;
pub mod wire;

/// A single message in a conversation.
#[derive(Debug, Clone)]
pub struct Message {
    pub role: Role,
    pub content: String,
}

/// The role of a message sender.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Role {
    User,
    Assistant,
}

/// A request to a model provider.
#[derive(Debug, Clone)]
pub struct CompletionRequest {
    pub model: String,
    pub system_prompt: String,
    /// Conversation messages (transcript + current prompt).
    pub messages: Vec<Message>,
}

/// The result of a completion request.
#[derive(Debug, Clone)]
pub struct CompletionResponse {
    pub content: String,
}

/// Errors from a model provider, with exit-code semantics.
#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    /// The request timed out. Maps to exit code 3.
    #[error("model request timed out")]
    Timeout,

    /// An HTTP or connection error. Maps to exit code 4.
    #[error("remote call failed: {0}")]
    RemoteCallFailed(String),

    /// The provider is not configured (missing API key). Maps to exit code 1.
    #[error("provider not configured: {0}")]
    NotConfigured(String),

    /// Any other error. Maps to exit code 1.
    #[error("{0}")]
    Other(String),
}

impl ProviderError {
    /// The exit code this error maps to per the clank spec.
    pub fn exit_code(&self) -> i32 {
        match self {
            ProviderError::Timeout => 3,
            ProviderError::RemoteCallFailed(_) => 4,
            ProviderError::NotConfigured(_) => 1,
            ProviderError::Other(_) => 1,
        }
    }
}

/// Abstraction over AI model providers.
#[async_trait]
pub trait ModelProvider: Send + Sync {
    async fn complete(
        &self,
        request: CompletionRequest,
    ) -> Result<CompletionResponse, ProviderError>;
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_error_timeout_maps_exit_3() {
        assert_eq!(ProviderError::Timeout.exit_code(), 3);
    }

    #[test]
    fn test_provider_error_http_maps_exit_4() {
        assert_eq!(
            ProviderError::RemoteCallFailed("connection refused".into()).exit_code(),
            4
        );
    }

    #[test]
    fn test_provider_error_not_configured_maps_exit_1() {
        assert_eq!(
            ProviderError::NotConfigured("anthropic".into()).exit_code(),
            1
        );
    }
}
