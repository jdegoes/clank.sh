/// HTTP client abstraction for clank.sh.
///
/// Provides a single [`HttpClient`] trait with platform-specific implementations:
/// - [`NativeHttpClient`] — backed by `reqwest` for native targets
/// - [`WasiHttpClient`]   — stub for `wasm32-wasip2` (WASM seam, future task)
///
/// Call sites hold `Arc<dyn HttpClient>` — no `#[cfg(target_arch)]` needed.
#[cfg(not(target_arch = "wasm32"))]
mod native;

#[cfg(target_arch = "wasm32")]
mod wasm;

#[cfg(not(target_arch = "wasm32"))]
pub use native::NativeHttpClient;

#[cfg(target_arch = "wasm32")]
pub use wasm::WasiHttpClient;

use async_trait::async_trait;
use thiserror::Error;

// ── Named types ───────────────────────────────────────────────────────────────

/// A single HTTP request header.
#[derive(Debug, Clone)]
pub struct RequestHeader {
    /// Header name (e.g. `"Authorization"`).
    pub name: String,
    /// Header value (e.g. `"Bearer sk-..."`).
    pub value: String,
}

impl RequestHeader {
    /// Convenience constructor.
    pub fn new(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value: value.into(),
        }
    }
}

/// The response from an HTTP request.
#[derive(Debug)]
pub struct HttpResponse {
    /// HTTP status code.
    pub status: u16,
    /// Response body decoded as UTF-8.
    pub body: String,
}

/// An error from an outgoing HTTP request.
#[derive(Debug, Error)]
pub enum HttpError {
    #[error("request failed: {0}")]
    Request(String),
    #[error("response body could not be decoded: {0}")]
    Decode(String),
    #[error("HTTP client not available on this target")]
    Unavailable,
}

// ── Trait ─────────────────────────────────────────────────────────────────────

/// Abstraction over outgoing HTTP, allowing the same call sites to work on
/// both native and `wasm32-wasip2` targets.
///
/// Implementations are selected at compile time via Cargo target-specific
/// dependencies. Call sites hold `Arc<dyn HttpClient>`.
#[async_trait]
pub trait HttpClient: Send + Sync {
    /// POST a JSON body to `url` with the given headers.
    /// Returns the response status and body on success.
    async fn post_json(
        &self,
        url: &str,
        headers: &[RequestHeader],
        body: &str,
    ) -> Result<HttpResponse, HttpError>;
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_header_new() {
        let h = RequestHeader::new("Authorization", "Bearer sk-test");
        assert_eq!(h.name, "Authorization");
        assert_eq!(h.value, "Bearer sk-test");
    }

    #[test]
    fn http_response_fields() {
        let r = HttpResponse {
            status: 200,
            body: "ok".to_string(),
        };
        assert_eq!(r.status, 200);
        assert_eq!(r.body, "ok");
    }

    #[test]
    fn http_error_display() {
        assert!(HttpError::Request("timeout".into())
            .to_string()
            .contains("timeout"));
        assert!(HttpError::Unavailable
            .to_string()
            .contains("not available"));
    }
}
