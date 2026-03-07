use std::sync::Arc;

use anyhow::Result;
use clank_http::NativeHttpClient;
use clank_shell::{register_command, ClankShell};

use clank::processes::{AskProcess, ModelProcess};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::WARN.into()),
        )
        .with_writer(std::io::stderr)
        .init();

    let http: Arc<dyn clank_http::HttpClient> = Arc::new(NativeHttpClient::new());
    let mut shell = ClankShell::with_http(
        std::sync::Arc::new(std::sync::RwLock::new(clank_shell::Transcript::default())),
        Arc::clone(&http),
    )
    .await?;

    // Register real process implementations for AI commands.
    let transcript = shell.transcript();
    register_command(
        shell.shell_id(),
        "ask",
        Arc::new(AskProcess::new(Arc::clone(&http), Arc::clone(&transcript))),
    );
    register_command(shell.shell_id(), "model", Arc::new(ModelProcess));

    shell.run_interactive().await?;

    Ok(())
}
