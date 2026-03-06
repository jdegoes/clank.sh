//! clank.sh entry point.
//!
//! Initialises tracing, constructs the REPL, and runs until EOF or `exit`.

use anyhow::Result;
use clank_core::Repl;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialise tracing. Defaults to WARN unless RUST_LOG is set.
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .with_writer(std::io::stderr)
        .init();

    let mut repl = Repl::new().await?;
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    repl.run(stdin.lock(), stdout).await?;
    Ok(())
}
