//! LLM provider abstraction for clank.sh.
//!
//! This crate defines the provider interface and implements Ollama and
//! OpenRouter as the first two concrete backends.  Callers use
//! [`provider_from_config`] to obtain an [`AnyProvider`] configured from
//! `~/.config/ask/ask.toml`.
//!
//! All dispatch is static: providers are generic over `H: HttpClient` so no
//! vtable overhead is incurred.

pub mod config;
pub mod ollama;
pub mod openrouter;

use std::sync::Arc;

use clank_http::HttpClient;

pub use config::{load_config, ProviderConfig};

// ---------------------------------------------------------------------------
// Core types
// ---------------------------------------------------------------------------

/// A single message in a conversation.
pub struct Message {
    pub role: Role,
    pub content: String,
}

/// The role of a [`Message`] in a conversation.
pub enum Role {
    System,
    User,
    Assistant,
}

/// Errors produced by provider operations.
#[derive(Debug)]
pub enum ProviderError {
    /// Provider is not configured (missing `ask.toml` or required fields).
    ///
    /// The shell's `context summarize` handler maps this to exit code 2.
    NotConfigured(String),
    /// HTTP transport error (connection refused, DNS failure, etc.).
    ///
    /// Maps to exit code 4.
    Transport(String),
    /// The provider returned a non-success HTTP status code.
    ///
    /// Maps to exit code 4 (or exit 2 for 401).
    Status(u16),
    /// The provider's response body could not be parsed.
    ///
    /// Maps to exit code 4.
    Parse(String),
}

impl std::fmt::Display for ProviderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProviderError::NotConfigured(msg) => write!(f, "{msg}"),
            ProviderError::Transport(msg) => write!(f, "connection failed: {msg}"),
            ProviderError::Status(401) => write!(f, "authentication failed (check api key)"),
            ProviderError::Status(code) => write!(f, "provider returned HTTP {code}"),
            ProviderError::Parse(msg) => {
                write!(f, "could not parse provider response: {msg}")
            }
        }
    }
}

impl std::error::Error for ProviderError {}

// ---------------------------------------------------------------------------
// Provider dispatch — static via enum, avoids dyn async fn
// ---------------------------------------------------------------------------

/// A concrete provider obtained from [`provider_from_config`].
///
/// Uses a generic `H: HttpClient` parameter so all dispatch is static.
/// The enum itself handles the Ollama vs OpenRouter fork at runtime without
/// requiring a vtable.
pub enum AnyProvider<H> {
    Ollama(ollama::OllamaProvider<H>),
    OpenRouter(openrouter::OpenRouterProvider<H>),
}

impl<H: HttpClient> AnyProvider<H> {
    /// Call the configured provider with `messages` and return the model's
    /// text response.
    pub async fn complete(&self, messages: &[Message]) -> Result<String, ProviderError> {
        match self {
            AnyProvider::Ollama(p) => p.complete(messages).await,
            AnyProvider::OpenRouter(p) => p.complete(messages).await,
        }
    }
}

// ---------------------------------------------------------------------------
// Factory
// ---------------------------------------------------------------------------

/// Construct an [`AnyProvider`] from the current `~/.config/ask/ask.toml`.
///
/// Reads the config file on every call.  Returns
/// [`ProviderError::NotConfigured`] when the file is missing or a required
/// field is absent.
pub fn provider_from_config<H: HttpClient>(http: Arc<H>) -> Result<AnyProvider<H>, ProviderError> {
    let config = load_config()?;
    match config.provider.as_str() {
        "ollama" => Ok(AnyProvider::Ollama(ollama::OllamaProvider::new(
            http,
            config.ollama_base_url().to_owned(),
            config.model,
        ))),
        "openrouter" => {
            let api_key = config
                .openrouter_api_key
                .expect("validated in load_config; openrouter_api_key is present");
            Ok(AnyProvider::OpenRouter(
                openrouter::OpenRouterProvider::new(http, config.model, api_key),
            ))
        }
        other => Err(ProviderError::NotConfigured(format!(
            "unknown provider \"{other}\""
        ))),
    }
}
