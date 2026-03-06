//! Integration tests for `clank_http`.
//!
//! Defines a `MockHttpClient` that records calls and returns canned responses.
//! Used to verify the `HttpClient` trait contract without making real network
//! calls, and to demonstrate the injection pattern all callers should follow.

use async_trait::async_trait;
use clank_http::{HttpClient, HttpError, HttpResponse};
use std::sync::{Arc, Mutex};

// ---------------------------------------------------------------------------
// MockHttpClient
// ---------------------------------------------------------------------------

/// Recorded HTTP call for assertion in tests.
#[derive(Debug, Clone)]
pub enum RecordedCall {
    Get { url: String },
    PostJson { url: String, body: Vec<u8> },
}

/// A mock `HttpClient` that returns a fixed response and records every call.
pub struct MockHttpClient {
    pub calls: Arc<Mutex<Vec<RecordedCall>>>,
    pub response: HttpResponse,
}

impl MockHttpClient {
    pub fn new(status: u16, body: &str) -> Self {
        Self {
            calls: Arc::new(Mutex::new(Vec::new())),
            response: HttpResponse {
                status,
                body: body.as_bytes().to_vec(),
            },
        }
    }

    pub fn calls_made(&self) -> Vec<RecordedCall> {
        self.calls.lock().unwrap().clone()
    }
}

#[async_trait]
impl HttpClient for MockHttpClient {
    async fn get(&self, url: &str) -> Result<HttpResponse, HttpError> {
        self.calls.lock().unwrap().push(RecordedCall::Get {
            url: url.to_string(),
        });
        Ok(HttpResponse {
            status: self.response.status,
            body: self.response.body.clone(),
        })
    }

    async fn post_json(&self, url: &str, body: &[u8]) -> Result<HttpResponse, HttpError> {
        self.calls.lock().unwrap().push(RecordedCall::PostJson {
            url: url.to_string(),
            body: body.to_vec(),
        });
        Ok(HttpResponse {
            status: self.response.status,
            body: self.response.body.clone(),
        })
    }
}

// ---------------------------------------------------------------------------
// HttpClient trait contract tests
// ---------------------------------------------------------------------------

/// Verify that any `HttpClient` implementation returns the expected response.
/// Call this with each concrete implementation to test contract conformance.
async fn assert_get_returns_response(client: &dyn HttpClient, expected_status: u16) {
    let resp = client
        .get("https://example.com")
        .await
        .expect("get should succeed");
    assert_eq!(resp.status, expected_status);
}

async fn assert_post_json_returns_response(client: &dyn HttpClient, expected_status: u16) {
    let resp = client
        .post_json("https://example.com", b"{}")
        .await
        .expect("post_json should succeed");
    assert_eq!(resp.status, expected_status);
}

// ---------------------------------------------------------------------------
// MockHttpClient tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn mock_get_records_call() {
    let client = MockHttpClient::new(200, "ok");
    client
        .get("https://example.com/foo")
        .await
        .expect("get should succeed");
    let calls = client.calls_made();
    assert_eq!(calls.len(), 1);
    assert!(matches!(&calls[0], RecordedCall::Get { url } if url == "https://example.com/foo"));
}

#[tokio::test]
async fn mock_post_json_records_call() {
    let client = MockHttpClient::new(201, "created");
    client
        .post_json("https://example.com/bar", b"{\"x\":1}")
        .await
        .expect("post_json should succeed");
    let calls = client.calls_made();
    assert_eq!(calls.len(), 1);
    assert!(
        matches!(&calls[0], RecordedCall::PostJson { url, body } if url == "https://example.com/bar" && body == b"{\"x\":1}")
    );
}

#[tokio::test]
async fn mock_get_contract() {
    let client = MockHttpClient::new(200, "hello");
    assert_get_returns_response(&client, 200).await;
}

#[tokio::test]
async fn mock_post_json_contract() {
    let client = MockHttpClient::new(201, "created");
    assert_post_json_returns_response(&client, 201).await;
}

#[tokio::test]
async fn mock_body_str_roundtrip() {
    let client = MockHttpClient::new(200, "hello world");
    let resp = client
        .get("https://example.com")
        .await
        .expect("get should succeed");
    assert_eq!(resp.body_str(), "hello world");
}

#[tokio::test]
async fn mock_client_is_injectable_as_arc_dyn() {
    // Verify the intended usage pattern: Arc<dyn HttpClient>.
    let client: Arc<dyn HttpClient> = Arc::new(MockHttpClient::new(200, "ok"));
    let resp = client
        .get("https://example.com")
        .await
        .expect("get should succeed");
    assert_eq!(resp.status, 200);
}

// ---------------------------------------------------------------------------
// NativeHttpClient tests (native only)
// ---------------------------------------------------------------------------

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use clank_http::NativeHttpClient;

    #[test]
    fn native_client_constructs() {
        NativeHttpClient::new().expect("NativeHttpClient::new should succeed");
    }

    #[test]
    fn native_client_default_constructs() {
        let _client = NativeHttpClient::default();
    }

    #[tokio::test]
    async fn native_client_is_injectable_as_arc_dyn() {
        use clank_http::HttpClient;
        use std::sync::Arc;
        let _client: Arc<dyn HttpClient> = Arc::new(NativeHttpClient::default());
    }
}
