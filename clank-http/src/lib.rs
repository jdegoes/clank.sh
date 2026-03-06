//! HTTP abstraction layer for clank.sh.
//!
//! Provides a single [`HttpClient`] trait with two implementations:
//! - [`NativeHttpClient`] — backed by `reqwest` for native targets
//! - `WasiHttpClient` — backed by `wstd` for `wasm32-wasip2` (not yet implemented)
//!
//! All call sites program against `Arc<dyn HttpClient>` with no `#[cfg(target_arch)]` needed.

use async_trait::async_trait;
use thiserror::Error;

/// Errors that can occur during HTTP operations.
#[derive(Debug, Error)]
pub enum HttpError {
    #[error("HTTP request failed: {0}")]
    RequestFailed(String),
    #[error("HTTP client not available on this target")]
    Unavailable,
}

/// A minimal HTTP response.
#[derive(Debug)]
pub struct HttpResponse {
    pub status: u16,
    pub body: Vec<u8>,
}

impl HttpResponse {
    /// Returns the response body as a UTF-8 string, lossy.
    pub fn body_str(&self) -> std::borrow::Cow<'_, str> {
        String::from_utf8_lossy(&self.body)
    }
}

/// Abstraction over HTTP clients, allowing the same call sites to work on both
/// native and `wasm32-wasip2` targets.
#[async_trait]
pub trait HttpClient: Send + Sync {
    /// Perform a GET request and return the response.
    async fn get(&self, url: &str) -> Result<HttpResponse, HttpError>;

    /// Perform a POST request with a JSON body and return the response.
    async fn post_json(&self, url: &str, body: &[u8]) -> Result<HttpResponse, HttpError>;
}

// ---------------------------------------------------------------------------
// Native implementation
// ---------------------------------------------------------------------------

/// [`HttpClient`] implementation backed by `reqwest` for native targets.
#[cfg(not(target_arch = "wasm32"))]
pub struct NativeHttpClient {
    inner: reqwest::Client,
}

#[cfg(not(target_arch = "wasm32"))]
impl NativeHttpClient {
    /// Create a new `NativeHttpClient` with default settings.
    pub fn new() -> anyhow::Result<Self> {
        let inner = reqwest::Client::builder()
            .build()
            .map_err(|e| anyhow::anyhow!("Failed to build HTTP client: {e}"))?;
        Ok(Self { inner })
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Default for NativeHttpClient {
    fn default() -> Self {
        Self::new().expect("Failed to construct NativeHttpClient with default settings")
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[async_trait]
impl HttpClient for NativeHttpClient {
    async fn get(&self, url: &str) -> Result<HttpResponse, HttpError> {
        let resp = self
            .inner
            .get(url)
            .send()
            .await
            .map_err(|e| HttpError::RequestFailed(e.to_string()))?;
        let status = resp.status().as_u16();
        let body = resp
            .bytes()
            .await
            .map_err(|e| HttpError::RequestFailed(e.to_string()))?
            .to_vec();
        Ok(HttpResponse { status, body })
    }

    async fn post_json(&self, url: &str, body: &[u8]) -> Result<HttpResponse, HttpError> {
        let resp = self
            .inner
            .post(url)
            .header("Content-Type", "application/json")
            .body(body.to_vec())
            .send()
            .await
            .map_err(|e| HttpError::RequestFailed(e.to_string()))?;
        let status = resp.status().as_u16();
        let body = resp
            .bytes()
            .await
            .map_err(|e| HttpError::RequestFailed(e.to_string()))?
            .to_vec();
        Ok(HttpResponse { status, body })
    }
}

// ---------------------------------------------------------------------------
// WASM stub — placeholder until the WASM process model is implemented
// ---------------------------------------------------------------------------

/// Stub [`HttpClient`] for `wasm32-wasip2`. Always returns [`HttpError::Unavailable`].
///
/// This type marks the seam where a real `wstd`-backed implementation will be
/// substituted once the WASM target is addressed in a subsequent task.
#[cfg(target_arch = "wasm32")]
pub struct WasiHttpClient;

#[cfg(target_arch = "wasm32")]
#[async_trait]
impl HttpClient for WasiHttpClient {
    async fn get(&self, _url: &str) -> Result<HttpResponse, HttpError> {
        Err(HttpError::Unavailable)
    }

    async fn post_json(&self, _url: &str, _body: &[u8]) -> Result<HttpResponse, HttpError> {
        Err(HttpError::Unavailable)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn http_error_display() {
        let e = HttpError::RequestFailed("timeout".into());
        assert!(e.to_string().contains("timeout"));

        let e = HttpError::Unavailable;
        assert!(e.to_string().contains("not available"));
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn native_client_constructs() {
        let _client = NativeHttpClient::new().expect("should construct");
    }
}
