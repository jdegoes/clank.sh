use std::sync::Arc;

#[tokio::main]
async fn main() {
    let shell = clank::build_shell().await;
    let http: Arc<dyn clank_http::HttpClient> = Arc::new(
        clank_http::NativeHttpClient::new()
            .expect("failed to build HTTP client"),
    );
    clank::run_repl(shell, http).await;
}
