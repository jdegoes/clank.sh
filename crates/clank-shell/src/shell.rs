use std::io::{self, BufRead as _, Write};
use std::sync::{Arc, RwLock};

use anyhow::Result;
use brush_builtins::{BuiltinSet, ShellBuilderExt as _};
use brush_core::Shell;

use clank_http::HttpClient;
use clank_vfs::{LayeredVfs, Vfs};

use crate::builtins::{
    clank_builtins, deregister_all, next_shell_id, register_command, set_active_shell,
    set_execution_context, set_sudo_state, ExecutionContext,
};
use crate::commands::{
    cat::CatProcess, env_cmd::EnvProcess, export::ExportProcess, grep::GrepProcess, ls::LsProcess,
    mkdir::MkdirProcess, prompt_user::PromptUserProcess, ps::PsProcess, rm::RmProcess,
    stat_cmd::StatProcess, touch::TouchProcess,
};
use crate::context_process::ContextProcess;
use crate::process::{Process, StubProcess};
use crate::transcript::{EntryKind, Transcript};

/// Map a `brush_core::ExecutionExitCode` to an `i32` process exit code.
///
/// This helper is used in two places:
/// - `run_line` — for the exit code returned to the shell loop.
/// - `ExitShell` — for `std::process::exit`.
///
/// Note: `Custom(u8)` is brush-core's representation; exit codes above 255
/// cannot be preserved through Brush. The mapping is consistent: Custom(n)
/// becomes n as i32, all other variants become 1.
pub(crate) fn exit_code_to_i32(code: brush_core::ExecutionExitCode) -> i32 {
    match code {
        brush_core::ExecutionExitCode::Success => 0,
        brush_core::ExecutionExitCode::Custom(n) => n as i32,
        _ => 1,
    }
}

/// The clank shell — wraps a `brush_core::Shell` with clank's process
/// dispatch layer and session transcript.
pub struct ClankShell {
    inner: Shell,
    transcript: Arc<RwLock<Transcript>>,
    shell_id: u64,
    vfs: Arc<dyn Vfs>,
    proc_snapshot: Arc<RwLock<Vec<clank_vfs::proc_handler::ProcessSnapshot>>>,
}

impl Drop for ClankShell {
    fn drop(&mut self) {
        // Remove all dispatch table, transcript, and sudo state entries for
        // this shell instance, preventing unbounded growth of global maps in
        // long-running processes and test suites.
        deregister_all(self.shell_id);
    }
}

impl ClankShell {
    /// Create a new `ClankShell` with a fresh transcript and a stub HTTP client
    /// for `context summarize`. Use `with_http` for a real HTTP client.
    pub async fn new() -> Result<Self> {
        use clank_http::NativeHttpClient;
        Self::with_http(
            Arc::new(RwLock::new(Transcript::default())),
            Arc::new(NativeHttpClient::new()),
        )
        .await
    }

    /// Create a new `ClankShell` sharing an existing transcript.
    /// Used in tests. `http` is injected for `context summarize`.
    pub async fn with_transcript(transcript: Arc<RwLock<Transcript>>) -> Result<Self> {
        use clank_http::NativeHttpClient;
        Self::with_http(transcript, Arc::new(NativeHttpClient::new())).await
    }

    /// Full constructor: transcript + HTTP client.
    pub async fn with_http(
        transcript: Arc<RwLock<Transcript>>,
        http: Arc<dyn HttpClient>,
    ) -> Result<Self> {
        let shell_id = next_shell_id();
        let stub: Arc<dyn Process + Send + Sync> = Arc::new(StubProcess { type_name: "stub" });

        // Initialise the global manifest registry if not already done.
        clank_manifest::init_global_registry();

        let shell = Shell::builder()
            .default_builtins(BuiltinSet::BashMode)
            .builtins(clank_builtins(shell_id, Arc::clone(&transcript), stub))
            .build()
            .await?;

        // Build the process snapshot source for the /proc/ handler.
        // Updated on each run_line() call.
        let proc_snapshot: Arc<RwLock<Vec<clank_vfs::proc_handler::ProcessSnapshot>>> =
            Arc::new(RwLock::new(Vec::new()));

        // Build the layered VFS with /proc/ mounted.
        let proc_handler = clank_vfs::proc_handler::ProcHandler::new(Arc::clone(&proc_snapshot));
        let vfs: Arc<dyn Vfs> = Arc::new(LayeredVfs::new().mount("/proc", proc_handler));

        // Register real process implementations replacing stubs.
        register_command(
            shell_id,
            "context",
            Arc::new(ContextProcess::new(
                Arc::clone(&transcript),
                Arc::clone(&http),
            )),
        );
        register_command(shell_id, "ls", Arc::new(LsProcess::new(Arc::clone(&vfs))));
        register_command(shell_id, "cat", Arc::new(CatProcess::new(Arc::clone(&vfs))));
        register_command(
            shell_id,
            "grep",
            Arc::new(GrepProcess::new(Arc::clone(&vfs))),
        );
        register_command(
            shell_id,
            "stat",
            Arc::new(StatProcess::new(Arc::clone(&vfs))),
        );
        register_command(
            shell_id,
            "mkdir",
            Arc::new(MkdirProcess::new(Arc::clone(&vfs))),
        );
        register_command(shell_id, "rm", Arc::new(RmProcess::new(Arc::clone(&vfs))));
        register_command(
            shell_id,
            "touch",
            Arc::new(TouchProcess::new(Arc::clone(&vfs))),
        );
        register_command(shell_id, "env", Arc::new(EnvProcess));
        register_command(shell_id, "ps", Arc::new(PsProcess::new(shell_id)));
        register_command(shell_id, "export", Arc::new(ExportProcess));
        register_command(
            shell_id,
            "prompt-user",
            Arc::new(PromptUserProcess::new(shell_id)),
        );

        Ok(Self {
            inner: shell,
            transcript,
            shell_id,
            vfs,
            proc_snapshot,
        })
    }

    /// Returns a shared reference to the transcript.
    pub fn transcript(&self) -> Arc<RwLock<Transcript>> {
        Arc::clone(&self.transcript)
    }

    /// Returns a shared reference to the VFS.
    pub fn vfs(&self) -> Arc<dyn Vfs> {
        Arc::clone(&self.vfs)
    }

    /// Returns the shell's unique ID. Used by callers that need to register
    /// commands in the global dispatch table for this specific shell instance.
    pub fn shell_id(&self) -> u64 {
        self.shell_id
    }

    /// Run a single line of shell input as a human user.
    ///
    /// Authorization policies (`Confirm`, `SudoOnly`) are not enforced — the
    /// user has already authorised the command by typing it.
    pub async fn run_line(&mut self, line: &str) -> i32 {
        self.run_line_with_context(line, ExecutionContext::User)
            .await
    }

    /// Run a single line of shell input as an AI agent.
    ///
    /// `Confirm` and `SudoOnly` authorization policies are enforced. This is
    /// the entry point for Phase 3 agent-issued commands.
    pub async fn run_line_as_agent(&mut self, line: &str) -> i32 {
        self.run_line_with_context(line, ExecutionContext::Agent)
            .await
    }

    /// Internal implementation shared by `run_line` and `run_line_as_agent`.
    async fn run_line_with_context(&mut self, line: &str, context: ExecutionContext) -> i32 {
        // Record the command before execution.
        {
            let mut t = self.transcript.write().expect("transcript lock poisoned");
            t.append(EntryKind::Command, line.trim(), false);
        }

        // Refresh the /proc/ snapshot from the live process table.
        {
            use crate::secrets::SecretsRegistry;
            use clank_vfs::proc_handler::ProcessSnapshot;

            // Build the environment snapshot once (filter out secret variable names).
            let secret_keys = SecretsRegistry::snapshot();
            let environ: Vec<(String, String)> = std::env::vars()
                .filter(|(k, _)| !secret_keys.contains(k))
                .collect();

            let entries = crate::process_table::snapshot(self.shell_id);
            let snapshots: Vec<ProcessSnapshot> = entries
                .iter()
                .map(|e| ProcessSnapshot {
                    pid: e.pid,
                    ppid: e.ppid,
                    argv: e.argv.clone(),
                    state_char: e.status.state_char(),
                    environ: environ.clone(),
                })
                .collect();
            *self
                .proc_snapshot
                .write()
                .expect("proc snapshot lock poisoned") = snapshots;
        }

        // Set the active shell ID and execution context so dispatch_builtin
        // can route to this shell and apply the correct authorization policy.
        set_active_shell(self.shell_id);
        set_execution_context(context);

        // Authorization check: compute `effective_line` (sudo prefix stripped
        // for user context) and enforce policies for agent context.
        //
        // Per spec (README.md § Authorization):
        //   "Agents cannot use sudo. An agent that needs elevation must pause
        //    and surface a confirmation request."
        //
        // Therefore in Agent context:
        //   - A `sudo` prefix is an immediate deny (exit 5) — agents have no
        //     sudo capability; elevation comes only from `sudo ask` at the
        //     human level, which is a separate concept not handled here.
        //   - SudoOnly commands are always denied regardless of prefix.
        //   - Confirm commands are handled in dispatch_builtin.
        //
        // In User context:
        //   - `sudo` prefix strips and grants elevation for the inner command.
        //   - No policy enforcement — the human authorises by typing.
        //
        // TODO(Phase 3 — agent context): remove the context guards once agent
        // execution is wired through run_line_as_agent in production.
        // See: dev-docs/issues/open/authorization-context-user-vs-agent.md
        let effective_line: String = {
            use clank_manifest::{AuthorizationPolicy, GLOBAL_REGISTRY};
            let cmd_name = line.split_whitespace().next().unwrap_or("");
            let is_sudo = cmd_name == "sudo";

            if is_sudo && context == ExecutionContext::Agent {
                // Agents cannot use sudo. Deny immediately.
                eprintln!("clank: agents cannot use sudo (exit 5)");
                return 5;
            }

            let (effective_cmd, stripped_line) = if is_sudo {
                // User context: strip prefix and grant elevation.
                let rest = line.trim_start_matches("sudo").trim_start();
                let inner = rest.split_whitespace().next().unwrap_or("");
                set_sudo_state(self.shell_id, true);
                (inner, rest)
            } else {
                (cmd_name, line)
            };

            if context == ExecutionContext::Agent {
                let policy = GLOBAL_REGISTRY
                    .read()
                    .expect("manifest registry poisoned")
                    .get(effective_cmd)
                    .map(|m| m.authorization_policy.clone())
                    .unwrap_or(AuthorizationPolicy::Allow);

                if policy == AuthorizationPolicy::SudoOnly {
                    // SudoOnly commands are always denied in agent context —
                    // agents cannot obtain sudo elevation.
                    eprintln!(
                        "clank: '{}' requires sudo authorization (exit 5)",
                        effective_cmd
                    );
                    return 5;
                }
            }

            stripped_line.to_string()
        };

        // Determine whether to capture stdout for the transcript.
        //
        // Shell-internal commands (ExecutionScope::ShellInternal, ParentShell) write
        // directly to the terminal and must NOT have their output re-recorded — doing
        // so would pollute the context window with transcript introspection output.
        //
        // All other commands (registered subprocess commands and OS-fallthrough) use
        // dual-path capture: set_fd on the per-invocation params AND replace_open_files
        // on the shell's persistent table. This ensures OS-spawned processes that
        // resolve fd 1 via shell.persistent_open_files() are captured even if they
        // don't see the per-params override.
        let is_internal = {
            use clank_manifest::ExecutionScope;
            let cmd_name = effective_line.split_whitespace().next().unwrap_or("");
            clank_manifest::GLOBAL_REGISTRY
                .read()
                .expect("manifest registry poisoned")
                .get(cmd_name)
                .map(|m| {
                    m.execution_scope == ExecutionScope::ShellInternal
                        || m.execution_scope == ExecutionScope::ParentShell
                })
                .unwrap_or(false) // unknown commands are treated as subprocess — capture
        };

        let (result, output) = if is_internal {
            // Shell-internal: output goes directly to real stdout; do not capture.
            let params = self.inner.default_exec_params();
            let result = self.inner.run_string(effective_line, &params).await;
            (result, String::new())
        } else {
            // Subprocess / OS-fallthrough: dual-path stdout capture.
            let tmp = tempfile::NamedTempFile::new().expect("failed to create capture temp file");
            let tmp_path = tmp.path().to_owned();
            let capture_file = tmp.reopen().expect("failed to reopen capture temp file");

            // Path 1: set_fd on params — picked up by registered builtins via
            // ExecutionContext::try_fd.
            let mut params = self.inner.default_exec_params();
            params.set_fd(
                brush_core::openfiles::OpenFiles::STDOUT_FD,
                brush_core::openfiles::OpenFile::from(
                    capture_file
                        .try_clone()
                        .expect("failed to clone capture file for params"),
                ),
            );

            // Path 2: replace_open_files on the shell's persistent fd table —
            // picked up by OS-spawned subprocesses that resolve fd 1 via the
            // shell's persistent table rather than params. We must preserve all
            // three standard fds since replace_open_files replaces the entire struct.
            self.inner.replace_open_files(
                [
                    (
                        brush_core::openfiles::OpenFiles::STDIN_FD,
                        brush_core::openfiles::OpenFile::Stdin(std::io::stdin()),
                    ),
                    (
                        brush_core::openfiles::OpenFiles::STDOUT_FD,
                        brush_core::openfiles::OpenFile::from(capture_file),
                    ),
                    (
                        brush_core::openfiles::OpenFiles::STDERR_FD,
                        brush_core::openfiles::OpenFile::Stderr(std::io::stderr()),
                    ),
                ]
                .into_iter(),
            );

            let result = self.inner.run_string(effective_line, &params).await;

            // Restore persistent fds to the real terminal before tee-ing output.
            self.inner.replace_open_files(
                [
                    (
                        brush_core::openfiles::OpenFiles::STDIN_FD,
                        brush_core::openfiles::OpenFile::Stdin(std::io::stdin()),
                    ),
                    (
                        brush_core::openfiles::OpenFiles::STDOUT_FD,
                        brush_core::openfiles::OpenFile::Stdout(std::io::stdout()),
                    ),
                    (
                        brush_core::openfiles::OpenFiles::STDERR_FD,
                        brush_core::openfiles::OpenFile::Stderr(std::io::stderr()),
                    ),
                ]
                .into_iter(),
            );

            let output = std::fs::read_to_string(&tmp_path).unwrap_or_else(|e| {
                tracing::warn!("failed to read subprocess capture file: {e}");
                String::new()
            });
            (result, output)
        };

        // Clear sudo state unconditionally after run_string returns, so that
        // parse errors cannot leave the flag set for the next command.
        set_sudo_state(self.shell_id, false);

        // Tee captured output to real stdout and record in transcript.
        if !output.is_empty() {
            print!("{output}");
            let _ = io::stdout().flush();
            let mut t = self.transcript.write().expect("transcript lock poisoned");
            t.append(EntryKind::Output, output.trim_end(), false);
        }

        match result {
            Ok(r) => match r.exit_code {
                brush_core::ExecutionExitCode::Success => 0,
                brush_core::ExecutionExitCode::Custom(n) => n as i32,
                _ => {
                    if matches!(
                        r.next_control_flow,
                        brush_core::ExecutionControlFlow::ExitShell
                    ) {
                        // Use exit_code_to_i32 consistently — same mapping as
                        // dispatch_builtin uses, not a second `as u8` conversion.
                        std::process::exit(exit_code_to_i32(r.exit_code));
                    }
                    1
                }
            },
            Err(e) => {
                eprintln!("clank: {e}");
                1
            }
        }
    }

    /// Returns true if the parse result indicates the command is incomplete.
    fn needs_more_input(
        result: Result<brush_parser::ast::Program, brush_parser::ParseError>,
    ) -> bool {
        match result {
            Err(brush_parser::ParseError::ParsingAtEndOfInput) => true,
            Err(brush_parser::ParseError::Tokenizing { ref inner, .. }) => {
                let msg = inner.to_string();
                msg.contains("unterminated here document")
                    || msg.contains("unterminated single quote")
                    || msg.contains("unterminated double quote")
                    || msg.contains("unterminated backquote")
                    || msg.contains("unterminated escape")
            }
            _ => false,
        }
    }

    /// Run the interactive read-eval-print loop, reading from stdin.
    ///
    /// Each line read is wrapped in `tokio::task::spawn_blocking` so that the
    /// blocking `read_line` syscall does not occupy the async executor thread
    /// while the user is typing.
    pub async fn run_interactive(&mut self) -> Result<()> {
        let stdout = io::stdout();
        let mut buf = String::new();

        loop {
            {
                let mut out = stdout.lock();
                if buf.is_empty() {
                    out.write_all(b"$ ")?;
                } else {
                    out.write_all(b"> ")?;
                }
                out.flush()?;
            }

            // Read one line without blocking the Tokio executor thread.
            let read_result =
                tokio::task::spawn_blocking(move || -> std::io::Result<(String, usize)> {
                    let mut line = String::new();
                    let n = std::io::stdin().lock().read_line(&mut line)?;
                    Ok((line, n))
                })
                .await
                .expect("spawn_blocking panicked");

            match read_result {
                Ok((_, 0)) => {
                    // EOF
                    if !buf.is_empty() {
                        self.run_line(buf.trim_end_matches('\n')).await;
                    }
                    break;
                }
                Ok((line, _)) => {
                    buf.push_str(&line);
                    if Self::needs_more_input(self.inner.parse_string(buf.as_str())) {
                        continue;
                    }
                    let cmd = buf.trim_end_matches('\n').to_string();
                    buf.clear();
                    if !cmd.is_empty() {
                        self.run_line(&cmd).await;
                    }
                }
                Err(e) => {
                    eprintln!("clank: read error: {e}");
                    break;
                }
            }
        }

        Ok(())
    }
}
