use async_trait::async_trait;

use crate::{HttpClient, HttpError, HttpResponse, RequestHeader};

/// [`HttpClient`] implementation backed by `reqwest` for native targets.
///
/// Constructed once and held behind `Arc<dyn HttpClient>`. The underlying
/// `reqwest::Client` connection pool is reused across calls.
pub struct NativeHttpClient {
    inner: reqwest::Client,
}

impl NativeHttpClient {
    /// Create a new client with default settings.
    pub fn new() -> Result<Self, HttpError> {
        let inner = reqwest::Client::builder()
            .build()
            .map_err(|e| HttpError::Request(format!("failed to build HTTP client: {e}")))?;
        Ok(Self { inner })
    }
}

impl Default for NativeHttpClient {
    fn default() -> Self {
        Self::new().expect("failed to construct NativeHttpClient with default settings")
    }
}

#[async_trait]
impl HttpClient for NativeHttpClient {
    async fn post_json(
        &self,
        url: &str,
        headers: &[RequestHeader],
        body: &str,
    ) -> Result<HttpResponse, HttpError> {
        let mut builder = self
            .inner
            .post(url)
            .header("Content-Type", "application/json")
            .body(body.to_string());

        for h in headers {
            builder = builder.header(&h.name, &h.value);
        }

        let response = builder
            .send()
            .await
            .map_err(|e| HttpError::Request(e.to_string()))?;

        let status = response.status().as_u16();

        let body = response
            .text()
            .await
            .map_err(|e| HttpError::Decode(e.to_string()))?;

        Ok(HttpResponse { status, body })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn native_client_constructs() {
        let _client = NativeHttpClient::new().expect("should construct");
    }

    #[test]
    fn native_client_is_arc_compatible() {
        let client: Arc<dyn HttpClient> =
            Arc::new(NativeHttpClient::new().expect("should construct"));
        // Just verifying the trait object compiles and can be held behind Arc.
        let _ = client;
    }

    #[tokio::test]
    async fn post_json_against_mock_server() {
        use mockito::Server;

        let mut server = Server::new_async().await;
        let mock = server
            .mock("POST", "/test")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"result":"ok"}"#)
            .create_async()
            .await;

        let client = NativeHttpClient::new().expect("should construct");
        let url = format!("{}/test", server.url());

        let response = client
            .post_json(&url, &[], r#"{"prompt":"hello"}"#)
            .await
            .expect("request should succeed");

        assert_eq!(response.status, 200);
        assert!(response.body.contains("ok"));

        mock.assert_async().await;
    }

    #[tokio::test]
    async fn post_json_sends_custom_headers() {
        use mockito::Server;

        let mut server = Server::new_async().await;
        let mock = server
            .mock("POST", "/headers")
            .match_header("x-api-key", "sk-test")
            .with_status(200)
            .with_body("ok")
            .create_async()
            .await;

        let client = NativeHttpClient::new().expect("should construct");
        let url = format!("{}/headers", server.url());
        let headers = vec![RequestHeader::new("x-api-key", "sk-test")];

        client
            .post_json(&url, &headers, "{}")
            .await
            .expect("request should succeed");

        mock.assert_async().await;
    }
}
