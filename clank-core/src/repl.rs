//! Minimal read-eval-print loop backed by `brush-core`.
//!
//! This is a thin driving layer — not a replacement for the full transcript-
//! aware interactive layer described in the README. That is a future task.
//! The goal here is to prove that `brush-core` integrates correctly and that
//! the shell can parse and execute commands end-to-end.

use anyhow::Result;

// ---------------------------------------------------------------------------
// Native implementation
// ---------------------------------------------------------------------------

/// A minimal REPL that drives a `brush_core::Shell`.
///
/// Reads lines from `stdin`, executes each as a shell command, and writes
/// output to `stdout` / `stderr` via the normal process streams.
///
/// # Note on `exit`
///
/// When the user types `exit`, brush-core's `exit` builtin calls
/// `std::process::exit` directly (it is a special builtin that terminates the
/// process). This means `run()` will not return normally on `exit`; the
/// process exits immediately. The acceptance test for "exit exits 0" relies
/// on this behaviour.
#[cfg(not(target_arch = "wasm32"))]
pub struct Repl {
    shell: brush_core::Shell,
}

#[cfg(not(target_arch = "wasm32"))]
impl Repl {
    /// Create a new `Repl` with a default `brush-core` shell configuration.
    pub async fn new() -> Result<Self> {
        use brush_builtins::{BuiltinSet, ShellBuilderExt as _};

        let shell = brush_core::Shell::builder()
            .default_builtins(BuiltinSet::BashMode)
            .shell_name("clank".to_string())
            .shell_product_display_str("clank.sh".to_string())
            .no_profile(true)
            .no_rc(true)
            .interactive(true)
            .build()
            .await?;
        Ok(Self { shell })
    }

    /// Run the REPL until EOF or `exit`.
    ///
    /// Reads from `stdin` line-by-line. Each non-empty line is executed as a
    /// shell command via `brush_core::Shell::run_string`.
    pub async fn run(&mut self) -> Result<()> {
        use std::io::{self, BufRead, Write};

        let stdin = io::stdin();
        let stdout = io::stdout();

        loop {
            // Print prompt to stdout (no transcript yet — that is a future task).
            {
                let mut out = stdout.lock();
                write!(out, "$ ")?;
                out.flush()?;
            }

            let mut line = String::new();
            let bytes_read = stdin.lock().read_line(&mut line)?;

            // EOF
            if bytes_read == 0 {
                break;
            }

            let trimmed = line.trim_end_matches('\n').trim_end_matches('\r');
            if trimmed.is_empty() {
                continue;
            }

            // Add to history so `!!` and `!n` work.
            let _ = self.shell.add_to_history(trimmed);

            let params = self.shell.default_exec_params();
            match self.shell.run_string(trimmed, &params).await {
                Ok(_result) => {}
                Err(err) => {
                    // Print errors to stderr, matching shell convention.
                    eprintln!("clank: {err}");
                }
            }
        }

        // Run exit hooks (e.g. EXIT traps).
        self.shell.on_exit().await?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// WASM stub
// ---------------------------------------------------------------------------

/// Stub `Repl` for `wasm32-wasip2`.
///
/// The WASM process model is not yet implemented. This type exists to keep
/// the crate structure consistent across targets. See the open issue for the
/// WASM process model design.
#[cfg(target_arch = "wasm32")]
pub struct Repl;

#[cfg(target_arch = "wasm32")]
impl Repl {
    pub async fn new() -> anyhow::Result<Self> {
        Ok(Self)
    }

    pub async fn run(&mut self) -> anyhow::Result<()> {
        eprintln!("clank: shell interpreter not yet implemented for wasm32 target");
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    #[cfg(not(target_arch = "wasm32"))]
    #[tokio::test]
    async fn repl_constructs() {
        super::Repl::new().await.expect("Repl::new should succeed");
    }
}
