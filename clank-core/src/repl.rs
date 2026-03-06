//! Minimal read-eval-print loop backed by `brush-core`.
//!
//! This is a thin driving layer — not a replacement for the full
//! transcript-aware interactive layer described in the README. The goal here
//! is to provide a working REPL that records every command and its output in
//! the [`Transcript`].

use anyhow::Result;
use std::sync::{Arc, Mutex};

use crate::transcript::{EntryKind, Transcript};

// ---------------------------------------------------------------------------
// Native implementation
// ---------------------------------------------------------------------------

/// A minimal REPL that drives a `brush_core::Shell` and records every
/// interaction in a [`Transcript`].
///
/// ## I/O
///
/// - `input` (passed to [`Repl::run`]) — source of command lines. Use
///   `io::stdin().lock()` in production or a `Cursor<&[u8]>` in tests.
/// - `prompt_out` (passed to [`Repl::run`]) — destination for the `$ ` prompt.
/// - Command stdout and stderr are captured via OS pipes (`TeeStream`) and
///   appended to the transcript, while still being forwarded to the real
///   process stdout/stderr so the user sees output normally.
///
/// ## Note on `exit`
///
/// When the user types `exit`, brush-core's `exit` builtin calls
/// `std::process::exit` directly. `run()` will not return normally — the
/// process exits immediately.
#[cfg(not(target_arch = "wasm32"))]
pub struct Repl {
    shell: brush_core::Shell,
    transcript: Arc<Mutex<Transcript>>,
}

#[cfg(not(target_arch = "wasm32"))]
impl Repl {
    /// Create a new `Repl` with a default `brush-core` shell configuration
    /// and a fresh transcript (default token budget).
    pub async fn new() -> Result<Self> {
        Ok(Self::with_transcript(
            Arc::new(Mutex::new(Transcript::default_budget())),
        )
        .await?)
    }

    /// Create a new `Repl` sharing the given transcript. Useful in tests that
    /// need to inspect the transcript after driving the REPL.
    ///
    /// Marks the current end of the transcript as the session start, so that
    /// entries appended by this REPL can be distinguished from any pre-existing
    /// entries via [`Transcript::session_entries`].
    pub async fn with_transcript(transcript: Arc<Mutex<Transcript>>) -> Result<Self> {
        use brush_builtins::{BuiltinSet, ShellBuilderExt as _};

        transcript.lock().unwrap().mark_session_start();

        let shell = brush_core::Shell::builder()
            .default_builtins(BuiltinSet::BashMode)
            .shell_name("clank".to_string())
            .shell_product_display_str("clank.sh".to_string())
            .no_profile(true)
            .no_rc(true)
            .interactive(true)
            .build()
            .await?;
        Ok(Self { shell, transcript })
    }

    /// Return a clone of the transcript handle.
    pub fn transcript(&self) -> Arc<Mutex<Transcript>> {
        Arc::clone(&self.transcript)
    }

    /// Run the REPL until EOF or `exit`.
    ///
    /// - `input`: source of command lines.
    /// - `prompt_out`: destination for the `$ ` prompt.
    pub async fn run(
        &mut self,
        input: impl std::io::BufRead,
        mut prompt_out: impl std::io::Write,
    ) -> Result<()> {
        use brush_core::openfiles::{OpenFile, OpenFiles};
        use crate::tee::{capture_stdout, capture_stderr};

        for line in input.lines() {
            write!(prompt_out, "$ ")?;
            prompt_out.flush()?;

            let line = line?;
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            // Record the input line in the transcript.
            self.transcript
                .lock()
                .unwrap()
                .append(EntryKind::Input, trimmed);

            // Add to brush-core history so `!!` and `!n` work.
            let _ = self.shell.add_to_history(trimmed);

            // Set up pipe-based capture for stdout and stderr.
            let (writer_out, handle_out) = capture_stdout()?;
            let (writer_err, handle_err) = capture_stderr()?;

            let mut params = self.shell.default_exec_params();
            params.set_fd(OpenFiles::STDOUT_FD, OpenFile::PipeWriter(writer_out));
            params.set_fd(OpenFiles::STDERR_FD, OpenFile::PipeWriter(writer_err));

            let result = self.shell.run_string(trimmed, &params).await;

            // Drop params — this closes the write ends of both pipes, sending
            // EOF to the drain threads so they can finish.
            drop(params);

            // Join the drain threads to retrieve captured text.
            let stdout_text = handle_out.join();
            let stderr_text = handle_err.join();

            if !stdout_text.is_empty() {
                self.transcript
                    .lock()
                    .unwrap()
                    .append(EntryKind::Output, stdout_text);
            }
            if !stderr_text.is_empty() {
                self.transcript
                    .lock()
                    .unwrap()
                    .append(EntryKind::Error, stderr_text);
            }

            if let Err(err) = result {
                eprintln!("clank: {err}");
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
/// the crate structure consistent across targets.
#[cfg(target_arch = "wasm32")]
pub struct Repl;

#[cfg(target_arch = "wasm32")]
impl Repl {
    pub async fn new() -> anyhow::Result<Self> {
        Ok(Self)
    }

    pub async fn with_transcript(
        _transcript: std::sync::Arc<std::sync::Mutex<crate::transcript::Transcript>>,
    ) -> anyhow::Result<Self> {
        Ok(Self)
    }

    pub fn transcript(
        &self,
    ) -> std::sync::Arc<std::sync::Mutex<crate::transcript::Transcript>> {
        std::sync::Arc::new(std::sync::Mutex::new(
            crate::transcript::Transcript::default_budget(),
        ))
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

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    #[tokio::test]
    async fn repl_constructs() {
        Repl::new().await.expect("Repl::new should succeed");
    }

    #[tokio::test]
    async fn repl_with_transcript_shares_handle() {
        let t = Arc::new(Mutex::new(Transcript::default_budget()));
        let repl = Repl::with_transcript(Arc::clone(&t))
            .await
            .expect("Repl::with_transcript should succeed");
        assert!(Arc::ptr_eq(&t, &repl.transcript()));
    }
}
