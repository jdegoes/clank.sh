use async_trait::async_trait;

use crate::{HttpClient, HttpError, HttpResponse, RequestHeader};

/// [`HttpClient`] stub for `wasm32-wasip2`.
///
/// Always returns [`HttpError::Unavailable`]. This marks the WASM HTTP seam
/// described in the README — the real implementation will use `golem-wasi-http`
/// once the WASM target is addressed in a subsequent task.
pub struct WasiHttpClient;

#[async_trait]
impl HttpClient for WasiHttpClient {
    async fn post_json(
        &self,
        _url: &str,
        _headers: &[RequestHeader],
        _body: &str,
    ) -> Result<HttpResponse, HttpError> {
        Err(HttpError::Unavailable)
    }
}
