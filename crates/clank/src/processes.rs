/// Thin `Process` adapters that bridge `clank-shell`'s `Process` trait with
/// the pure functions in `clank-ask`. These live in the `clank` binary crate
/// because it is the only crate that depends on both `clank-shell` and
/// `clank-ask`, avoiding a circular dependency.
use std::sync::{Arc, RwLock};

use async_trait::async_trait;

use clank_ask::{run_ask, run_model, AskOutput, ModelOutput};
use clank_http::HttpClient;
use clank_shell::{
    process::{Process, ProcessContext, ProcessResult},
    Transcript,
};

/// `Process` adapter for the `ask` command.
pub struct AskProcess {
    http: Arc<dyn HttpClient>,
    transcript: Arc<RwLock<Transcript>>,
}

impl AskProcess {
    pub fn new(http: Arc<dyn HttpClient>, transcript: Arc<RwLock<Transcript>>) -> Self {
        Self { http, transcript }
    }
}

#[async_trait]
impl Process for AskProcess {
    async fn run(&self, mut ctx: ProcessContext) -> ProcessResult {
        // Read transcript window for model context.
        let transcript_text = self
            .transcript
            .read()
            .expect("transcript lock poisoned")
            .format_for_model();

        // Read piped stdin only if stdin is actually a pipe — never block on
        // the terminal. read_piped_stdin() returns None for terminal stdin.
        let piped: Vec<u8> = ctx
            .io
            .read_piped_stdin()
            .unwrap_or(None)
            .unwrap_or_default();

        let cwd = std::env::current_dir()
            .ok()
            .map(|p| p.to_string_lossy().to_string());

        let AskOutput {
            stdout,
            stderr,
            exit_code,
        } = run_ask(
            &ctx.argv,
            piped,
            Arc::clone(&self.http),
            transcript_text,
            cwd.as_deref(),
            None, // load real config
        )
        .await;

        if !stdout.is_empty() {
            let _ = ctx.io.write_stdout(stdout.as_bytes());
            // Append AI response to transcript.
            self.transcript
                .write()
                .expect("transcript lock poisoned")
                .append(clank_shell::EntryKind::AiResponse, stdout.trim(), false);
        }
        if !stderr.is_empty() {
            let _ = ctx.io.write_stderr(&stderr);
        }

        if exit_code == 0 {
            ProcessResult::success()
        } else {
            ProcessResult::failure(exit_code)
        }
    }
}

/// `Process` adapter for the `model` command.
pub struct ModelProcess;

#[async_trait]
impl Process for ModelProcess {
    async fn run(&self, mut ctx: ProcessContext) -> ProcessResult {
        let ModelOutput {
            stdout,
            stderr,
            exit_code,
        } = run_model(&ctx.argv);
        if !stdout.is_empty() {
            let _ = ctx.io.write_stdout(stdout.as_bytes());
        }
        if !stderr.is_empty() {
            let _ = ctx.io.write_stderr(stderr.as_bytes());
        }
        if exit_code == 0 {
            ProcessResult::success()
        } else {
            ProcessResult::failure(exit_code)
        }
    }
}
