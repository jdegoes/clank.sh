use std::collections::HashMap;
use std::io::{BufRead as _, Write as _};
use std::sync::{Arc, LazyLock, RwLock};

use brush_core::builtins::Registration;
use brush_core::env::{EnvironmentLookup, EnvironmentScope};
use brush_core::variables::ShellValueLiteral;
use futures::future::BoxFuture;

use crate::process::{Process, ProcessContext, ProcessIo};
use crate::process_table::{self, ProcessStatus, ProcessType};
use crate::secrets::SecretsRegistry;
use crate::transcript::Transcript;

// ---------------------------------------------------------------------------
// Global dispatch table
// ---------------------------------------------------------------------------
//
// `CommandExecuteFunc` is a bare fn pointer — it cannot capture variables.
// We store the dispatch table keyed by (shell_id, command_name) so that
// multiple independent shell instances (e.g. in tests) do not share state.
//
// shell_id is a monotonically increasing u64 assigned at ClankShell creation.

use std::sync::atomic::{AtomicU64, Ordering};

/// Monotonically increasing shell ID counter.
/// `Relaxed` ordering is correct: `fetch_add` is an atomic RMW, which
/// guarantees each caller receives a unique value regardless of memory
/// ordering. No cross-thread synchronisation is required beyond uniqueness.
static NEXT_SHELL_ID: AtomicU64 = AtomicU64::new(1);

/// Allocate a new unique shell ID.
pub fn next_shell_id() -> u64 {
    NEXT_SHELL_ID.fetch_add(1, Ordering::Relaxed)
}

type DispatchKey = (u64, String);
type DispatchTable = HashMap<DispatchKey, Arc<dyn Process + Send + Sync>>;

// Transcript table: shell_id → transcript
type TranscriptTable = HashMap<u64, Arc<RwLock<Transcript>>>;

// Per-shell sudo authorization state.
// Keyed by shell_id so that concurrent ClankShell instances do not share
// authorization state. This mirrors the DISPATCH table design.
type SudoStateTable = HashMap<u64, bool>;

static DISPATCH: LazyLock<RwLock<DispatchTable>> = LazyLock::new(|| RwLock::new(HashMap::new()));
static TRANSCRIPTS: LazyLock<RwLock<TranscriptTable>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));
static SUDO_STATE: LazyLock<RwLock<SudoStateTable>> = LazyLock::new(|| RwLock::new(HashMap::new()));

/// Whether the current command was issued by a human user or an AI agent.
///
/// In Phase 1 all commands originate from user input; `User` is the default.
/// Phase 3 will introduce an agent execution path that sets `Agent`, at which
/// point `Confirm` and `SudoOnly` policies will be enforced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionContext {
    /// Command typed directly by the human user. Authorization policies are
    /// not enforced — the user has already authorised by typing the command.
    User,
    /// Command issued autonomously by an AI agent. `Confirm` and `SudoOnly`
    /// policies are enforced to protect the user from unintended actions.
    Agent,
}

// The active shell_id and execution context are communicated to the bare fn
// pointer via thread-locals. Set both before calling run_string.
thread_local! {
    pub static ACTIVE_SHELL_ID: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    pub static ACTIVE_EXECUTION_CONTEXT: std::cell::Cell<ExecutionContext> =
        const { std::cell::Cell::new(ExecutionContext::User) };
}

/// Set the execution context for the current thread.
/// Must be called before `run_string` alongside `set_active_shell`.
pub fn set_execution_context(ctx: ExecutionContext) {
    ACTIVE_EXECUTION_CONTEXT.with(|c| c.set(ctx));
}

/// Get the execution context for the current thread.
pub fn get_execution_context() -> ExecutionContext {
    ACTIVE_EXECUTION_CONTEXT.with(|c| c.get())
}

/// Set the active shell ID for the current thread.
/// Must be called before `run_string` and ideally cleared after.
pub fn set_active_shell(id: u64) {
    ACTIVE_SHELL_ID.with(|c| c.set(id));
}

/// Set the sudo authorization state for a specific shell instance.
pub fn set_sudo_state(shell_id: u64, val: bool) {
    SUDO_STATE
        .write()
        .expect("sudo state table poisoned")
        .insert(shell_id, val);
}

/// Get the sudo authorization state for a specific shell instance.
pub fn get_sudo_state(shell_id: u64) -> bool {
    *SUDO_STATE
        .read()
        .expect("sudo state table poisoned")
        .get(&shell_id)
        .unwrap_or(&false)
}

/// Register a command for a specific shell instance.
pub fn register_command(
    shell_id: u64,
    name: impl Into<String>,
    process: Arc<dyn Process + Send + Sync>,
) {
    DISPATCH
        .write()
        .expect("dispatch table poisoned")
        .insert((shell_id, name.into()), process);
}

/// Deregister a command for a specific shell instance.
pub fn deregister_command(shell_id: u64, name: &str) {
    DISPATCH
        .write()
        .expect("dispatch table poisoned")
        .remove(&(shell_id, name.to_string()));
}

/// Remove all dispatch, transcript, and sudo state entries for a shell instance.
/// Called from `ClankShell::drop` to prevent unbounded growth of global maps.
pub fn deregister_all(shell_id: u64) {
    DISPATCH
        .write()
        .expect("dispatch table poisoned")
        .retain(|(id, _), _| *id != shell_id);

    TRANSCRIPTS
        .write()
        .expect("transcript table poisoned")
        .remove(&shell_id);

    SUDO_STATE
        .write()
        .expect("sudo state table poisoned")
        .remove(&shell_id);
}

/// Register the transcript for a shell instance.
pub fn set_transcript(shell_id: u64, transcript: Arc<RwLock<Transcript>>) {
    TRANSCRIPTS
        .write()
        .expect("transcript table poisoned")
        .insert(shell_id, transcript);
}

/// Retrieve the transcript for the currently active shell.
pub fn get_transcript() -> Option<Arc<RwLock<Transcript>>> {
    let id = ACTIVE_SHELL_ID.with(|c| c.get());
    if id == 0 {
        return None;
    }
    TRANSCRIPTS
        .read()
        .expect("transcript table poisoned")
        .get(&id)
        .map(Arc::clone)
}

/// Look up a process for the currently active shell.
fn lookup(name: &str) -> Option<Arc<dyn Process + Send + Sync>> {
    let id = ACTIVE_SHELL_ID.with(|c| c.get());
    DISPATCH
        .read()
        .expect("dispatch table poisoned")
        .get(&(id, name.to_string()))
        .map(Arc::clone)
}

/// Return a snapshot of the current environment, with secret variable values
/// masked as `***`. Used to populate `ProcessContext.env` for each command.
pub fn current_env_snapshot() -> HashMap<String, String> {
    let secrets = SecretsRegistry::snapshot();
    std::env::vars()
        .map(|(k, v)| {
            if secrets.contains(k.as_str()) {
                (k, "***".to_string())
            } else {
                (k, v)
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Initial registration
// ---------------------------------------------------------------------------

/// Populate the dispatch table for a shell instance and return the
/// corresponding `Registration` entries for Brush.
pub fn clank_builtins(
    shell_id: u64,
    transcript: Arc<RwLock<Transcript>>,
    stub: Arc<dyn Process + Send + Sync>,
) -> HashMap<String, Registration> {
    set_transcript(shell_id, transcript);

    let commands: &[&str] = &[
        "context",
        "prompt-user",
        "export",
        "ls",
        "cat",
        "cp",
        "mv",
        "rm",
        "mkdir",
        "touch",
        "find",
        "grep",
        "sed",
        "awk",
        "sort",
        "uniq",
        "wc",
        "head",
        "tail",
        "cut",
        "tr",
        "xargs",
        "diff",
        "patch",
        "tee",
        "stat",
        "file",
        "jq",
        "curl",
        "wget",
        "env",
        "ps",
        "kill",
        "man",
        "ask",
        "model",
        "mcp",
        "golem",
        "grease",
    ];

    {
        let mut guard = DISPATCH.write().expect("dispatch table poisoned");
        for &name in commands {
            guard.insert((shell_id, name.to_string()), Arc::clone(&stub));
        }
    }

    let mut map = HashMap::new();
    for &name in commands {
        map.insert(
            name.to_string(),
            Registration {
                execute_func: dispatch_builtin,
                content_func: empty_content,
                disabled: false,
                special_builtin: false,
                // `export` must be a declaration builtin so Brush routes
                // assignment-style args (KEY=value) as CommandArg::Assignment.
                declaration_builtin: name == "export",
            },
        );
    }

    map
}

// ---------------------------------------------------------------------------
// Brush fn-pointer callbacks
// ---------------------------------------------------------------------------

#[allow(clippy::result_large_err)]
fn dispatch_builtin(
    ctx: brush_core::commands::ExecutionContext<'_>,
    args: Vec<brush_core::CommandArg>,
) -> BoxFuture<'_, Result<brush_core::ExecutionResult, brush_core::Error>> {
    let cmd_name = ctx.command_name.clone();
    let process = lookup(&cmd_name);
    let io = ProcessIo::from_context(&ctx);
    // Capture Brush's working directory before the async move consumes ctx.
    // This is the source of truth for relative path resolution in VFS commands —
    // NOT std::env::current_dir(), which is the OS process cwd and is never
    // updated when `cd` runs.
    let cwd = ctx.shell.working_dir().to_path_buf();

    let shell_id = ACTIVE_SHELL_ID.with(|c| c.get());

    Box::pin(async move {
        // Dev 1: For `export`, perform real env mutation before any other dispatch.
        // Brush's declaration-builtin machinery passes assignment args as
        // CommandArg::Assignment with already-expanded name/value pairs.
        if cmd_name == "export" {
            let secret = args
                .iter()
                .any(|a| matches!(a, brush_core::CommandArg::String(s) if s == "--secret"));
            for arg in &args {
                if let brush_core::CommandArg::Assignment(a) = arg {
                    let name = a.name.to_string();
                    let value = a.value.to_string();
                    let _ = ctx.shell.env.update_or_add(
                        name.clone(),
                        ShellValueLiteral::Scalar(value),
                        |v| {
                            v.export();
                            Ok(())
                        },
                        EnvironmentLookup::Anywhere,
                        EnvironmentScope::Global,
                    );
                    if secret {
                        SecretsRegistry::insert(&name);
                    }
                }
            }
        }

        // Brush passes args including argv[0] (the command name).
        let argv: Vec<String> = args
            .into_iter()
            .map(|a| match a {
                brush_core::CommandArg::String(s) => s,
                brush_core::CommandArg::Assignment(a) => a.to_string(),
            })
            .collect();

        // Determine process type from the manifest registry.
        let process_type = {
            use clank_manifest::{ExecutionScope, GLOBAL_REGISTRY};
            let cmd = argv.first().map(String::as_str).unwrap_or(&cmd_name);
            GLOBAL_REGISTRY
                .read()
                .expect("manifest registry poisoned")
                .get(cmd)
                .map(|m| match m.execution_scope {
                    ExecutionScope::ParentShell => ProcessType::ParentShell,
                    ExecutionScope::ShellInternal => ProcessType::ShellInternal,
                    ExecutionScope::Subprocess => ProcessType::Subprocess,
                })
                .unwrap_or(ProcessType::Subprocess)
        };

        // Register in the process table.
        let pid = process_table::spawn(shell_id, 0, argv.clone(), process_type);

        // Enforce Confirm authorization policy in Agent context only.
        // Sudo state was set by run_line_with_context if the user prefixed with `sudo`.
        //
        // TODO(Phase 3 — agent context): remove the `execution_context == Agent` guard
        // if a "safe mode" for user-typed input is introduced.
        // See: dev-docs/issues/open/authorization-context-user-vs-agent.md
        {
            use clank_manifest::{AuthorizationPolicy, GLOBAL_REGISTRY};

            let execution_context = get_execution_context();

            if execution_context == ExecutionContext::Agent {
                let policy = GLOBAL_REGISTRY
                    .read()
                    .expect("manifest registry poisoned")
                    .get(&cmd_name)
                    .map(|m| m.authorization_policy.clone())
                    .unwrap_or(AuthorizationPolicy::Allow);

                if matches!(policy, AuthorizationPolicy::Confirm) && !get_sudo_state(shell_id) {
                    // Enter P state before presenting the confirmation prompt.
                    process_table::set_status(shell_id, pid, ProcessStatus::Paused);

                    {
                        let stderr = std::io::stderr();
                        let mut err = stderr.lock();
                        let _ = write!(err, "{cmd_name} requires confirmation. (y)es, (n)o: ");
                        let _ = err.flush();
                    }

                    // Read the answer on a blocking thread so we do not stall the
                    // Tokio executor. This matches the pattern used in run_interactive.
                    let ans = tokio::task::spawn_blocking(|| {
                        let mut line = String::new();
                        let _ = std::io::stdin().lock().read_line(&mut line);
                        line.trim().to_lowercase()
                    })
                    .await
                    .unwrap_or_default();

                    // Restore Running state regardless of answer.
                    process_table::set_status(shell_id, pid, ProcessStatus::Running);

                    if ans != "y" && ans != "yes" {
                        let stderr = std::io::stderr();
                        let mut err = stderr.lock();
                        let _ = writeln!(err, "clank: aborted.");
                        process_table::complete(shell_id, pid, 1);
                        process_table::reap(shell_id, pid);
                        return Ok(brush_core::ExecutionResult {
                            exit_code: brush_core::ExecutionExitCode::Custom(1),
                            next_control_flow: brush_core::ExecutionControlFlow::Normal,
                        });
                    }
                }
            }
            // Note: sudo state is cleared in run_line_with_context after run_string
            // returns, not here. This ensures parse errors cannot leave the flag set.
        }

        let exit_code = if let Some(process) = process {
            let result = process
                .run(ProcessContext {
                    argv,
                    // Populate the environment snapshot so commands like `env`
                    // see the actual exported variables, with secrets masked.
                    env: current_env_snapshot(),
                    io,
                    pid,
                    cwd: cwd.clone(),
                })
                .await;
            // Complete the process table entry.
            process_table::complete(shell_id, pid, result.exit_code);
            process_table::reap(shell_id, pid);
            // Map exit code to ExecutionExitCode. Exit code 0 must map to
            // Success (not Custom(0)) so that `&&` short-circuit logic works
            // correctly. Non-zero exit codes use Custom so that run_line can
            // preserve the exact numeric value.
            if result.exit_code == 0 {
                brush_core::ExecutionExitCode::Success
            } else {
                // Clamp to u8 range — brush_core::ExecutionExitCode::Custom
                // takes u8. Saturate rather than wrap so that exit codes above
                // 255 become 255, not silently 0 (which would turn an error
                // into an apparent success).
                brush_core::ExecutionExitCode::Custom(result.exit_code.clamp(1, 255) as u8)
            }
        } else {
            process_table::complete(shell_id, pid, 127);
            process_table::reap(shell_id, pid);
            let mut stderr = ctx.stderr();
            let _ = std::io::Write::write_all(
                &mut stderr,
                format!("clank: {cmd_name}: not found in dispatch table\n").as_bytes(),
            );
            brush_core::ExecutionExitCode::Custom(127)
        };

        Ok(brush_core::ExecutionResult {
            exit_code,
            next_control_flow: brush_core::ExecutionControlFlow::Normal,
        })
    })
}

#[allow(clippy::result_large_err)]
fn empty_content(
    _name: &str,
    _content_type: brush_core::builtins::ContentType,
) -> Result<String, brush_core::Error> {
    Ok(String::new())
}
