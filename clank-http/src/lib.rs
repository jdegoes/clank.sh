//! HTTP client abstraction for clank.sh.
//!
//! This crate defines the [`HttpClient`] trait and the types it operates on.
//! Call sites never import a concrete client directly; they receive an
//! `Arc<dyn HttpClient>` injected at startup.  This keeps all
//! `#[cfg(target_arch)]` guards contained here and invisible to the rest of
//! the codebase.
//!
//! # Current status
//!
//! Only the native implementation ([`NativeHttpClient`]) exists.  The WASM
//! implementation is deferred until `brush-core` can compile to
//! `wasm32-wasip2` (see issue: dev-docs/issues/open/brush-wasm-portability.md
//! once filed).

use std::future::Future;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// A successful HTTP response.
#[derive(Debug)]
pub struct HttpResponse {
    /// HTTP status code.
    pub status: u16,
    /// Response body as raw bytes.
    pub body: Vec<u8>,
}

impl HttpResponse {
    /// Decode the body as UTF-8, returning an error if it is not valid UTF-8.
    pub fn text(&self) -> Result<&str, std::str::Utf8Error> {
        std::str::from_utf8(&self.body)
    }
}

/// Errors returned by [`HttpClient`] implementations.
#[derive(Debug)]
pub enum HttpError {
    /// A network-level or transport error.
    Transport(String),
    /// The server returned a non-success status code.
    Status(u16),
}

impl std::fmt::Display for HttpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HttpError::Transport(msg) => write!(f, "HTTP transport error: {msg}"),
            HttpError::Status(code) => write!(f, "HTTP error status: {code}"),
        }
    }
}

impl std::error::Error for HttpError {}

// ---------------------------------------------------------------------------
// Trait
// ---------------------------------------------------------------------------

/// An async HTTP client.
///
/// Implementations are injected via `Arc<dyn HttpClient + Send + Sync>` so
/// that callers are decoupled from the concrete transport.
pub trait HttpClient: Send + Sync {
    /// Issue a GET request and return the response.
    fn get(&self, url: &str) -> impl Future<Output = Result<HttpResponse, HttpError>> + Send;
}

// ---------------------------------------------------------------------------
// Native implementation
// ---------------------------------------------------------------------------

/// The native HTTP client, backed by [`reqwest`].
///
/// Not available on `wasm32` targets — use the WASM implementation once it is
/// implemented (see module-level documentation).
#[cfg(not(target_arch = "wasm32"))]
pub struct NativeHttpClient {
    inner: reqwest::Client,
}

#[cfg(not(target_arch = "wasm32"))]
impl NativeHttpClient {
    /// Create a new `NativeHttpClient` with default settings.
    pub fn new() -> Self {
        Self {
            inner: reqwest::Client::new(),
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Default for NativeHttpClient {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl HttpClient for NativeHttpClient {
    async fn get(&self, url: &str) -> Result<HttpResponse, HttpError> {
        let response = self
            .inner
            .get(url)
            .send()
            .await
            .map_err(|e| HttpError::Transport(e.to_string()))?;

        let status = response.status().as_u16();
        if !response.status().is_success() {
            return Err(HttpError::Status(status));
        }

        let body = response
            .bytes()
            .await
            .map_err(|e| HttpError::Transport(e.to_string()))?
            .to_vec();

        Ok(HttpResponse { status, body })
    }
}

// ---------------------------------------------------------------------------
// WASM stub — compile-time reminder
// ---------------------------------------------------------------------------

/// Placeholder for the future WASM HTTP client.
///
/// This type exists so that the pattern of selecting an implementation via
/// `#[cfg]` is established in the codebase.  It will be replaced with a real
/// implementation backed by `wstd` once `brush-core` is portable to
/// `wasm32-wasip2`.
///
/// See: dev-docs/issues/open/brush-wasm-portability.md (once filed).
#[cfg(target_arch = "wasm32")]
pub struct WasiHttpClient;

#[cfg(target_arch = "wasm32")]
impl HttpClient for WasiHttpClient {
    async fn get(&self, _url: &str) -> Result<HttpResponse, HttpError> {
        // WASM HTTP implementation is not yet available.
        // Track progress in: dev-docs/issues/open/brush-wasm-portability.md
        todo!("WasiHttpClient is not yet implemented; see brush-wasm-portability issue")
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Smoke-test that `NativeHttpClient` can reach a public URL and returns
    /// a successful response with a non-empty body.
    ///
    /// This test makes a real network request and is therefore ignored in
    /// offline / CI environments unless explicitly opted in with
    /// `-- --ignored`.
    #[tokio::test]
    #[ignore = "requires network access"]
    async fn native_client_get_succeeds() {
        let client = NativeHttpClient::new();
        let response = client
            .get("https://httpbin.org/get")
            .await
            .expect("GET request should succeed");
        assert_eq!(response.status, 200);
        assert!(!response.body.is_empty());
    }
}
