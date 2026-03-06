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
/// Reads lines from a `BufRead` source, executes each as a shell command, and
/// writes the prompt to a `Write` sink. Output from commands flows through the
/// normal process streams (stdout/stderr) since brush-core owns them directly.
///
/// The injectable I/O (`input` / `prompt_out`) is for the REPL control channel
/// only — reading the next command and writing the prompt. This separation
/// means tests can drive the REPL with in-memory buffers while still
/// capturing command output via `std::process::Command` at the binary level.
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
    /// - `input`: source of command lines (e.g. `io::stdin().lock()` or a
    ///   `Cursor<&[u8]>` in tests).
    /// - `prompt_out`: destination for the `$ ` prompt (e.g. `io::stdout()`
    ///   or a `Vec<u8>` in tests).
    ///
    /// Each non-empty line read from `input` is executed via
    /// `brush_core::Shell::run_string`. Execution errors are printed to
    /// stderr.
    pub async fn run(
        &mut self,
        input: impl std::io::BufRead,
        mut prompt_out: impl std::io::Write,
    ) -> Result<()> {
        for line in input.lines() {
            write!(prompt_out, "$ ")?;
            prompt_out.flush()?;

            let line = line?;
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            // Add to history so `!!` and `!n` work.
            let _ = self.shell.add_to_history(trimmed);

            let params = self.shell.default_exec_params();
            match self.shell.run_string(trimmed, &params).await {
                Ok(_result) => {}
                Err(err) => {
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

    pub async fn run(
        &mut self,
        _input: impl std::io::BufRead,
        _prompt_out: impl std::io::Write,
    ) -> anyhow::Result<()> {
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
