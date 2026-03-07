use async_trait::async_trait;

/// Errors that can occur during an HTTP request.
#[derive(Debug, thiserror::Error)]
pub enum HttpError {
    #[error("connection failed: {0}")]
    ConnectionFailed(String),

    #[error("request timed out")]
    Timeout,

    #[error("non-success response: status={status}, body={body}")]
    NonSuccessResponse { status: u16, body: String },

    #[error("TLS error: {0}")]
    Tls(String),
}

impl From<reqwest::Error> for HttpError {
    fn from(e: reqwest::Error) -> Self {
        if e.is_timeout() {
            HttpError::Timeout
        } else if e.is_connect() {
            HttpError::ConnectionFailed(e.to_string())
        } else if let Some(status) = e.status() {
            HttpError::NonSuccessResponse {
                status: status.as_u16(),
                body: e.to_string(),
            }
        } else {
            HttpError::ConnectionFailed(e.to_string())
        }
    }
}

/// A simple HTTP request.
#[derive(Debug)]
pub struct Request {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<Vec<u8>>,
}

/// A simple HTTP response.
#[derive(Debug)]
pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

/// Abstraction over HTTP client implementations.
///
/// All outbound HTTP in clank goes through this trait. The concrete implementation is
/// `NativeHttpClient` on native targets and will be `WasiHttpClient` on `wasm32-wasip2`
/// (added in Phase 0).
#[async_trait]
pub trait HttpClient: Send + Sync {
    async fn send(&self, req: Request) -> Result<Response, HttpError>;
}

/// A canned response to return from `MockHttpClient`.
#[derive(Debug, Clone)]
pub struct MockResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl MockResponse {
    /// Convenience constructor for a plain 200 JSON response.
    pub fn json(body: impl Into<String>) -> Self {
        Self {
            status: 200,
            headers: vec![("content-type".into(), "application/json".into())],
            body: body.into().into_bytes(),
        }
    }

    /// Convenience constructor for a plain 200 text response.
    pub fn text(body: impl Into<String>) -> Self {
        Self {
            status: 200,
            headers: vec![],
            body: body.into().into_bytes(),
        }
    }

    /// Convenience constructor for an error response.
    pub fn error(status: u16, body: impl Into<String>) -> Self {
        Self {
            status,
            headers: vec![],
            body: body.into().into_bytes(),
        }
    }
}

/// Test double for `HttpClient`.
///
/// Records all requests sent to it and returns pre-configured responses in
/// FIFO order. Panics if called when the response queue is empty.
///
/// Available to all crates as a `dev-dependency` on `clank-http`. Always
/// access via `clank_http::MockHttpClient`.
///
/// # Example
///
/// ```rust
/// # use clank_http::{MockHttpClient, MockResponse, Request, HttpClient};
/// # tokio_test::block_on(async {
/// let client = MockHttpClient::new(vec![MockResponse::json(r#"{"ok":true}"#)]);
/// let req = Request {
///     method: "GET".into(),
///     url: "https://example.com".into(),
///     headers: vec![],
///     body: None,
/// };
/// let resp = client.send(req).await.unwrap();
/// assert_eq!(resp.status, 200);
/// # });
/// ```
/// `MockHttpClient` uses `tokio::sync::Mutex` so that the compiler will catch
/// any future attempt to hold a lock guard across an `.await` point, which
/// would deadlock a single-threaded Tokio executor. Using a sync `Mutex` in
/// an async context is safe only when guards never span await points — using
/// Tokio's mutex makes that invariant enforced rather than merely documented.
pub struct MockHttpClient {
    responses: tokio::sync::Mutex<std::collections::VecDeque<Result<MockResponse, HttpError>>>,
    pub requests: tokio::sync::Mutex<Vec<Request>>,
}

impl MockHttpClient {
    /// Create a new mock that will return the given responses in order.
    pub fn new(responses: Vec<MockResponse>) -> Self {
        Self {
            responses: tokio::sync::Mutex::new(responses.into_iter().map(Ok).collect()),
            requests: tokio::sync::Mutex::new(Vec::new()),
        }
    }

    /// Create a new mock that will return the given results (allowing errors) in order.
    pub fn with_results(results: Vec<Result<MockResponse, HttpError>>) -> Self {
        Self {
            responses: tokio::sync::Mutex::new(results.into()),
            requests: tokio::sync::Mutex::new(Vec::new()),
        }
    }

    /// Return all recorded requests, consuming them (destructive).
    ///
    /// Named `take_recorded_requests` to make the destructive semantics
    /// explicit. Tests that need non-destructive access can use
    /// `mock.requests.lock().await` directly.
    pub async fn take_recorded_requests(&self) -> Vec<Request> {
        self.requests.lock().await.drain(..).collect()
    }
}

#[async_trait]
impl HttpClient for MockHttpClient {
    async fn send(&self, req: Request) -> Result<Response, HttpError> {
        self.requests.lock().await.push(req);
        let result = self
            .responses
            .lock()
            .await
            .pop_front()
            .expect("MockHttpClient: no more responses queued");
        result.map(|r| {
            if !(200..300).contains(&r.status) {
                // Automatically convert error-status mock responses into the
                // correct error variant, matching NativeHttpClient behaviour.
                return Err(HttpError::NonSuccessResponse {
                    status: r.status,
                    body: String::from_utf8_lossy(&r.body).to_string(),
                });
            }
            Ok(Response {
                status: r.status,
                headers: r.headers,
                body: r.body,
            })
        })?
    }
}

/// HTTP client backed by `reqwest`. Used on native targets.
pub struct NativeHttpClient {
    client: reqwest::Client,
}

impl NativeHttpClient {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .use_rustls_tls()
                .build()
                .expect("failed to build reqwest client"),
        }
    }
}

impl Default for NativeHttpClient {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl HttpClient for NativeHttpClient {
    async fn send(&self, req: Request) -> Result<Response, HttpError> {
        let method = reqwest::Method::from_bytes(req.method.as_bytes())
            .map_err(|e| HttpError::ConnectionFailed(e.to_string()))?;

        let mut builder = self.client.request(method, &req.url);
        for (key, value) in &req.headers {
            builder = builder.header(key.as_str(), value.as_str());
        }
        if let Some(body) = req.body {
            builder = builder.body(body);
        }

        let resp = builder.send().await.map_err(HttpError::from)?;
        let status = resp.status().as_u16();

        let headers = resp
            .headers()
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
            .collect();

        let body = resp.bytes().await.map_err(HttpError::from)?.to_vec();

        if !(200..300).contains(&status) {
            return Err(HttpError::NonSuccessResponse {
                status,
                body: String::from_utf8_lossy(&body).to_string(),
            });
        }

        Ok(Response {
            status,
            headers,
            body,
        })
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- HttpError display strings --

    #[test]
    fn test_http_error_display_timeout() {
        let e = HttpError::Timeout;
        assert_eq!(e.to_string(), "request timed out");
    }

    #[test]
    fn test_http_error_display_connection_failed() {
        let e = HttpError::ConnectionFailed("refused".to_string());
        assert_eq!(e.to_string(), "connection failed: refused");
    }

    #[test]
    fn test_http_error_display_non_success() {
        let e = HttpError::NonSuccessResponse {
            status: 404,
            body: "not found".to_string(),
        };
        assert_eq!(
            e.to_string(),
            "non-success response: status=404, body=not found"
        );
    }

    #[test]
    fn test_http_error_display_tls() {
        let e = HttpError::Tls("certificate expired".to_string());
        assert_eq!(e.to_string(), "TLS error: certificate expired");
    }

    // -- From<reqwest::Error> conversion --

    #[tokio::test]
    async fn test_http_error_from_reqwest_timeout() {
        // Build a client with a near-zero timeout so the request times out
        // immediately, giving us a genuine reqwest timeout error to convert.
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_nanos(1))
            .build()
            .unwrap();
        let err = client
            .get("http://203.0.113.1") // TEST-NET — guaranteed unreachable
            .send()
            .await
            .unwrap_err();
        assert!(err.is_timeout(), "expected a timeout error from reqwest");
        let mapped = HttpError::from(err);
        assert!(
            matches!(mapped, HttpError::Timeout),
            "timeout reqwest error must map to HttpError::Timeout"
        );
    }

    #[tokio::test]
    async fn test_http_error_from_reqwest_connect_failure() {
        // Port 1 on loopback is almost always closed; this produces a
        // connection-refused error, not a timeout.
        let client = reqwest::Client::new();
        let err = client.get("http://127.0.0.1:1/").send().await.unwrap_err();
        // reqwest classifies connection-refused as a connect error.
        let mapped = HttpError::from(err);
        assert!(
            matches!(mapped, HttpError::ConnectionFailed(_)),
            "connection error must map to HttpError::ConnectionFailed"
        );
    }

    // -- MockHttpClient behaviour --

    #[tokio::test]
    async fn test_mock_http_client_records_request_and_returns_response() {
        use std::sync::Arc;
        let mock = Arc::new(MockHttpClient::new(vec![MockResponse::json(
            r#"{"ok":true}"#,
        )]));
        let req = Request {
            method: "POST".to_string(),
            url: "https://example.com/api".to_string(),
            headers: vec![("x-test".to_string(), "value".to_string())],
            body: Some(b"hello".to_vec()),
        };
        let resp = mock.send(req).await.unwrap();
        assert_eq!(resp.status, 200);
        assert_eq!(resp.body, b"{\"ok\":true}");

        let reqs = mock.requests.lock().await;
        assert_eq!(reqs.len(), 1);
        assert_eq!(reqs[0].url, "https://example.com/api");
        assert_eq!(reqs[0].method, "POST");
    }

    #[tokio::test]
    async fn test_mock_http_client_non_2xx_via_new_converts_to_error() {
        // MockResponse::error() passed to new() should auto-convert to
        // HttpError::NonSuccessResponse on send().
        use std::sync::Arc;
        let mock = Arc::new(MockHttpClient::new(vec![MockResponse::error(
            503,
            "Service Unavailable",
        )]));
        let req = Request {
            method: "GET".to_string(),
            url: "https://example.com".to_string(),
            headers: vec![],
            body: None,
        };
        let err = mock.send(req).await.unwrap_err();
        assert!(
            matches!(err, HttpError::NonSuccessResponse { status: 503, .. }),
            "non-2xx MockResponse must become NonSuccessResponse, got: {err:?}"
        );
    }

    #[tokio::test]
    #[should_panic(expected = "MockHttpClient: no more responses queued")]
    async fn test_mock_http_client_panics_on_empty_queue() {
        use std::sync::Arc;
        let mock = Arc::new(MockHttpClient::new(vec![]));
        let req = Request {
            method: "GET".to_string(),
            url: "https://example.com".to_string(),
            headers: vec![],
            body: None,
        };
        let _ = mock.send(req).await;
    }
}
