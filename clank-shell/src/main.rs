//! clank.sh entry point.
//!
//! Initialises tracing, constructs the REPL, and runs until EOF or `exit`.
//!
//! # Flags
//!
//! `--dump-transcript <path>` — after the session ends, write the transcript
//! as JSON to `<path>`. Used by the golden test harness.

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

    // Parse --dump-transcript <path> from argv (minimal; no full CLI parser needed yet).
    let dump_transcript_path = parse_dump_transcript_arg();

    let mut repl = Repl::new().await?;
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    repl.run(stdin.lock(), stdout).await?;

    if let Some(path) = dump_transcript_path {
        let json = repl.transcript().lock().unwrap().session_to_json();
        std::fs::write(&path, json)
            .unwrap_or_else(|e| eprintln!("clank: failed to write transcript to {path:?}: {e}"));
    }

    Ok(())
}

/// Extract the value of `--dump-transcript <path>` from `std::env::args`, if present.
fn parse_dump_transcript_arg() -> Option<std::path::PathBuf> {
    let args: Vec<String> = std::env::args().collect();
    let pos = args.iter().position(|a| a == "--dump-transcript")?;
    args.get(pos + 1).map(std::path::PathBuf::from)
}
