use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use brush_core::openfiles::OpenFile;

/// Standard I/O handles passed to a process at invocation time.
///
/// These are real file descriptors sourced directly from the Brush
/// `ExecutionContext`, so they correctly carry pipe ends, redirections, and
/// any other file-descriptor manipulation the shell has applied before
/// dispatching the command.
pub struct ProcessIo {
    pub stdin: OpenFile,
    pub stdout: OpenFile,
    pub stderr: OpenFile,
}

impl ProcessIo {
    /// Construct `ProcessIo` from a Brush `ExecutionContext`.
    ///
    /// Falls back to the real process stdin/stdout/stderr if a file descriptor
    /// is not present in the context (which should not happen in normal use,
    /// but makes the API robust).
    pub fn from_context(ctx: &brush_core::commands::ExecutionContext<'_>) -> Self {
        use brush_core::openfiles::OpenFiles;
        Self {
            stdin: ctx
                .try_fd(OpenFiles::STDIN_FD)
                .unwrap_or_else(|| OpenFile::Stdin(std::io::stdin())),
            stdout: ctx
                .try_fd(OpenFiles::STDOUT_FD)
                .unwrap_or_else(|| OpenFile::Stdout(std::io::stdout())),
            stderr: ctx
                .try_fd(OpenFiles::STDERR_FD)
                .unwrap_or_else(|| OpenFile::Stderr(std::io::stderr())),
        }
    }

    /// Read all bytes from stdin, but only if stdin is a pipe.
    ///
    /// Returns `None` when stdin is the terminal (`OpenFile::Stdin`) — reading
    /// the terminal stdin in a non-interactive context would block indefinitely.
    /// Returns `Some(bytes)` when stdin is `OpenFile::PipeReader`, which means
    /// the caller piped data into this command (e.g. `cat file | ask "..."`).
    pub fn read_piped_stdin(&mut self) -> std::io::Result<Option<Vec<u8>>> {
        match &mut self.stdin {
            OpenFile::PipeReader(_) => {
                let mut buf = Vec::new();
                self.stdin.read_to_end(&mut buf)?;
                Ok(Some(buf))
            }
            _ => Ok(None),
        }
    }

    /// Read all bytes from stdin.
    pub fn read_stdin(&mut self) -> std::io::Result<Vec<u8>> {
        let mut buf = Vec::new();
        self.stdin.read_to_end(&mut buf)?;
        Ok(buf)
    }

    /// Write bytes to stdout.
    pub fn write_stdout(&mut self, data: &[u8]) -> std::io::Result<()> {
        self.stdout.write_all(data)
    }

    /// Write bytes to stderr.
    pub fn write_stderr(&mut self, data: &[u8]) -> std::io::Result<()> {
        self.stderr.write_all(data)
    }
}

/// Context passed to every `Process::run` invocation.
pub struct ProcessContext {
    /// Argument vector. `argv[0]` is the command name.
    pub argv: Vec<String>,

    /// Environment variables in scope at invocation time.
    pub env: HashMap<String, String>,

    /// Standard I/O handles for this invocation, correctly reflecting any
    /// pipe or redirection the shell applied before dispatch.
    pub io: ProcessIo,

    /// The PID assigned to this process in the process table.
    /// Set by `dispatch_builtin` after `process_table::spawn`.
    pub pid: u64,

    /// The shell's current working directory at the time of dispatch, as
    /// tracked by Brush. All relative paths in VFS commands must be resolved
    /// against this — never against `std::env::current_dir()`, which is the
    /// OS process cwd and is not updated when `cd` runs.
    pub cwd: PathBuf,
}

/// The result of running a process.
#[derive(Debug, Clone)]
pub struct ProcessResult {
    /// Shell exit code. 0 = success.
    pub exit_code: i32,
}

impl ProcessResult {
    pub fn success() -> Self {
        Self { exit_code: 0 }
    }

    pub fn failure(code: i32) -> Self {
        Self { exit_code: code }
    }
}

/// The core abstraction over all process types in clank.
///
/// Every command the shell can execute — builtins, scripts, prompts, Golem
/// agent invocations — is an implementation of this trait. Brush never reaches
/// its own OS process-spawning path because every known command name is
/// registered as a Brush builtin that dispatches here.
#[async_trait]
pub trait Process: Send + Sync {
    async fn run(&self, ctx: ProcessContext) -> ProcessResult;
}

// ---------------------------------------------------------------------------
// Stub implementation
// ---------------------------------------------------------------------------

/// A process type that has not yet been implemented.
///
/// Returns exit code 1 and writes a stable "not yet implemented" message to
/// the process's stderr handle (correctly respecting any redirection). Does
/// not panic.
pub struct StubProcess {
    pub type_name: &'static str,
}

#[async_trait]
impl Process for StubProcess {
    async fn run(&self, mut ctx: ProcessContext) -> ProcessResult {
        let cmd = ctx.argv.first().map(String::as_str).unwrap_or("<unknown>");
        let msg = format!("clank: {cmd}: not yet implemented ({})\n", self.type_name);
        // Write to the process's stderr handle — respects redirections.
        let _ = ctx.io.write_stderr(msg.as_bytes());
        ProcessResult::failure(1)
    }
}

/// Wraps a `Process` impl behind an `Arc` for shared ownership.
pub type SharedProcess = Arc<dyn Process>;
