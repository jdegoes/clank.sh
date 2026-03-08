//! clank-builtins — execution scope metadata and clank-owned builtin commands.
//!
//! This crate owns the `ExecutionScope` classification, the `CommandManifest`
//! type, the static manifest registry for all commands clank recognises, and
//! the implementations of clank-owned shell-internal builtins.

use std::io::Write as _;
use std::sync::Arc;

use brush_core::builtins::{simple_builtin, ContentType, Registration, SimpleCommand};
use brush_core::{commands::ExecutionContext, results::ExecutionResult};

/// The execution scope of a command, as defined in the clank architecture.
///
/// Every command resolvable by the shell has exactly one scope. Scope
/// determines routing, state-mutation rights, and AI tool-surface eligibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionScope {
    /// Runs in the parent shell context and may mutate shell state (cwd, env,
    /// function table). POSIX special builtins fall here. Cannot be invoked
    /// as a subprocess.
    ParentShell,

    /// Implemented entirely within the shell; operates on internal tables
    /// (alias table, job table, transcript, history). Cannot run as a
    /// subprocess.
    ShellInternal,

    /// Runs as an isolated subprocess with no access to parent shell state.
    /// Scripts, prompts, Golem agents, and installed executables all fall here.
    /// The only scope exposed to `ask` as AI tools.
    Subprocess,
}

/// Minimal command manifest entry for this foundational step.
///
/// Carries the command name, its execution scope, and the names of any
/// arguments whose values must be scrubbed from the transcript before storage
/// (`redaction_rules`). Empty slice means no argument-level redaction.
pub struct CommandManifest {
    pub name: &'static str,
    pub scope: ExecutionScope,
    /// Named flag arguments whose values must never appear in the transcript,
    /// `ps`, logs, or provider manifests. Example: `&["--key", "--token"]`.
    /// Heuristic regex redaction applies regardless of this field; this field
    /// is for values that must be redacted even if they don't match any pattern.
    pub redaction_rules: &'static [&'static str],
}

/// Static registry of all commands clank classifies by execution scope.
///
/// Commands not present here are unknown to the clank manifest layer; they
/// fall through to brush-core's default dispatch. The registry grows as
/// commands are added to the clank surface.
pub static MANIFEST_REGISTRY: &[CommandManifest] = &[
    // parent-shell: mutate shell state
    CommandManifest {
        name: ".",
        scope: ExecutionScope::ParentShell,
        redaction_rules: &[],
    },
    CommandManifest {
        name: "cd",
        scope: ExecutionScope::ParentShell,
        redaction_rules: &[],
    },
    CommandManifest {
        name: "exec",
        scope: ExecutionScope::ParentShell,
        redaction_rules: &[],
    },
    CommandManifest {
        name: "exit",
        scope: ExecutionScope::ParentShell,
        redaction_rules: &[],
    },
    CommandManifest {
        name: "export",
        scope: ExecutionScope::ParentShell,
        redaction_rules: &[],
    },
    CommandManifest {
        name: "source",
        scope: ExecutionScope::ParentShell,
        redaction_rules: &[],
    },
    CommandManifest {
        name: "unset",
        scope: ExecutionScope::ParentShell,
        redaction_rules: &[],
    },
    // shell-internal: operate on shell-owned tables
    CommandManifest {
        name: "alias",
        scope: ExecutionScope::ShellInternal,
        redaction_rules: &[],
    },
    CommandManifest {
        name: "bg",
        scope: ExecutionScope::ShellInternal,
        redaction_rules: &[],
    },
    CommandManifest {
        name: "context",
        scope: ExecutionScope::ShellInternal,
        redaction_rules: &[],
    },
    CommandManifest {
        name: "fg",
        scope: ExecutionScope::ShellInternal,
        redaction_rules: &[],
    },
    CommandManifest {
        name: "jobs",
        scope: ExecutionScope::ShellInternal,
        redaction_rules: &[],
    },
    CommandManifest {
        name: "read",
        scope: ExecutionScope::ShellInternal,
        redaction_rules: &[],
    },
    CommandManifest {
        name: "type",
        scope: ExecutionScope::ShellInternal,
        redaction_rules: &[],
    },
    CommandManifest {
        name: "unalias",
        scope: ExecutionScope::ShellInternal,
        redaction_rules: &[],
    },
    CommandManifest {
        name: "wait",
        scope: ExecutionScope::ShellInternal,
        redaction_rules: &[],
    },
    // subprocess: isolated execution
    CommandManifest {
        name: "ask",
        scope: ExecutionScope::Subprocess,
        redaction_rules: &[],
    },
    CommandManifest {
        name: "cat",
        scope: ExecutionScope::Subprocess,
        redaction_rules: &[],
    },
    CommandManifest {
        name: "curl",
        scope: ExecutionScope::Subprocess,
        redaction_rules: &[],
    },
    CommandManifest {
        name: "find",
        scope: ExecutionScope::Subprocess,
        redaction_rules: &[],
    },
    CommandManifest {
        name: "grep",
        scope: ExecutionScope::Subprocess,
        redaction_rules: &[],
    },
    CommandManifest {
        name: "ls",
        scope: ExecutionScope::Subprocess,
        redaction_rules: &[],
    },
];

/// Returns the execution scope of `name` if it is present in the manifest
/// registry, or `None` if the command is not yet classified by clank.
pub fn scope_of(name: &str) -> Option<ExecutionScope> {
    MANIFEST_REGISTRY
        .iter()
        .find(|m| m.name == name)
        .map(|m| m.scope)
}

/// Returns the redaction rules for `name` if it is present in the manifest
/// registry, or an empty slice if the command is unknown or has no rules.
pub fn redaction_rules_of(name: &str) -> &'static [&'static str] {
    MANIFEST_REGISTRY
        .iter()
        .find(|m| m.name == name)
        .map(|m| m.redaction_rules)
        .unwrap_or(&[])
}

// ---------------------------------------------------------------------------
// context builtin
// ---------------------------------------------------------------------------

/// Implementation of the `context` shell-internal builtin.
///
/// Subcommands: `show`, `clear`, `trim <n>`, `summarize`.
pub struct ContextBuiltin;

impl SimpleCommand for ContextBuiltin {
    fn get_content(name: &str, content_type: ContentType) -> Result<String, brush_core::Error> {
        let usage = format!("usage: {name} show|clear|trim <n>|summarize\n");
        match content_type {
            ContentType::ShortUsage | ContentType::DetailedHelp | ContentType::ManPage => Ok(usage),
            ContentType::ShortDescription => Ok(format!("{name} - manage the shell transcript\n")),
        }
    }

    fn execute<I: Iterator<Item = S>, S: AsRef<str>>(
        context: ExecutionContext<'_>,
        mut args: I,
    ) -> Result<ExecutionResult, brush_core::Error> {
        // The first element is the command name itself; skip it.
        args.next();

        let subcommand = match args.next() {
            Some(s) => s,
            None => {
                writeln!(
                    context.stderr(),
                    "context: usage: context show|clear|trim <n>"
                )
                .ok();
                return Ok(ExecutionResult::from(
                    brush_core::results::ExecutionExitCode::InvalidUsage,
                ));
            }
        };

        match subcommand.as_ref() {
            "summarize" => Ok(summarize_transcript(context)),
            "show" => {
                let timestamps = args.any(|a| a.as_ref() == "--timestamps");
                let transcript = clank_transcript::global();
                let locked = transcript.lock().unwrap_or_else(|e| e.into_inner());
                let mut stdout = context.stdout();
                for entry in locked.entries() {
                    let line = if timestamps {
                        entry.display_with_timestamps()
                    } else {
                        entry.display_plain()
                    };
                    writeln!(stdout, "{line}").ok();
                }
                Ok(ExecutionResult::success())
            }
            "clear" => {
                clank_transcript::global()
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .clear();
                Ok(ExecutionResult::success())
            }
            "trim" => {
                let n_str = match args.next() {
                    Some(s) => s,
                    None => {
                        writeln!(context.stderr(), "context: trim: missing argument <n>").ok();
                        return Ok(ExecutionResult::from(
                            brush_core::results::ExecutionExitCode::InvalidUsage,
                        ));
                    }
                };
                match n_str.as_ref().parse::<usize>() {
                    Ok(n) => {
                        clank_transcript::global().lock().unwrap().trim(n);
                        Ok(ExecutionResult::success())
                    }
                    Err(_) => {
                        writeln!(
                            context.stderr(),
                            "context: trim: invalid argument {:?}: expected non-negative integer",
                            n_str.as_ref()
                        )
                        .ok();
                        Ok(ExecutionResult::from(
                            brush_core::results::ExecutionExitCode::InvalidUsage,
                        ))
                    }
                }
            }
            other => {
                writeln!(
                    context.stderr(),
                    "context: unknown subcommand {other:?}: expected show, clear, trim, or summarize"
                )
                .ok();
                Ok(ExecutionResult::from(
                    brush_core::results::ExecutionExitCode::InvalidUsage,
                ))
            }
        }
    }
}

/// Implements `context summarize`: calls the configured LLM provider with the
/// current transcript and prints the resulting summary to stdout.
///
/// Returns [`ExecutionResult`] directly — all error handling is done
/// internally via stderr writes, so there is no need to propagate
/// `brush_core::Error` up the call stack.
fn summarize_transcript(context: ExecutionContext<'_>) -> ExecutionResult {
    use clank_http::NativeHttpClient;
    use clank_provider::{provider_from_config, Message, ProviderError, Role};

    // Check provider configuration before reading the transcript so that a
    // missing config is always reported as an error, even on an empty session.
    let http = Arc::new(NativeHttpClient::new());
    let provider = match provider_from_config(http) {
        Ok(p) => p,
        Err(ProviderError::NotConfigured(msg)) => {
            writeln!(context.stderr(), "context summarize: {msg}").ok();
            return ExecutionResult::from(brush_core::results::ExecutionExitCode::InvalidUsage);
        }
        Err(e) => {
            writeln!(context.stderr(), "context summarize: {e}").ok();
            return ExecutionResult::from(brush_core::results::ExecutionExitCode::Custom(4));
        }
    };

    // Collect transcript text after provider validation succeeds.
    let transcript_text = {
        let transcript = clank_transcript::global();
        let locked = transcript.lock().unwrap_or_else(|e| e.into_inner());
        locked
            .entries()
            .map(|e| e.display_plain())
            .collect::<Vec<_>>()
            .join("\n")
    };

    if transcript_text.is_empty() {
        writeln!(context.stdout(), "(transcript is empty)").ok();
        return ExecutionResult::success();
    }

    let messages = vec![
        Message {
            role: Role::System,
            content: "You are a summarization assistant. Produce a concise summary of the \
                      following shell session transcript. Output only the summary text, \
                      with no preamble."
                .into(),
        },
        Message {
            role: Role::User,
            content: transcript_text,
        },
    ];

    // Run the async provider call on a dedicated OS thread with its own
    // single-thread tokio runtime.  brush-core uses `Handle::current().block_on`
    // internally, so `block_in_place` on the same thread deadlocks.  A fresh
    // thread with a fresh runtime has no entanglement with the outer runtime.
    let complete_result = {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("provider runtime should build");
            let _ = tx.send(rt.block_on(provider.complete(&messages)));
        });
        rx.recv().unwrap_or_else(|_| {
            Err(clank_provider::ProviderError::Transport(
                "provider thread panicked".into(),
            ))
        })
    };
    match complete_result {
        Ok(summary) => {
            writeln!(context.stdout(), "{summary}").ok();
            ExecutionResult::success()
        }
        Err(ProviderError::NotConfigured(msg)) => {
            writeln!(context.stderr(), "context summarize: {msg}").ok();
            ExecutionResult::from(brush_core::results::ExecutionExitCode::InvalidUsage)
        }
        Err(ProviderError::Status(401)) => {
            writeln!(
                context.stderr(),
                "context summarize: authentication failed (check api key)"
            )
            .ok();
            ExecutionResult::from(brush_core::results::ExecutionExitCode::InvalidUsage)
        }
        Err(e) => {
            writeln!(context.stderr(), "context summarize: {e}").ok();
            ExecutionResult::from(brush_core::results::ExecutionExitCode::Custom(4))
        }
    }
}

/// Returns a brush-core `Registration` for the `context` builtin.
///
/// Called by `clank_core::default_options()` to register the builtin at
/// shell construction time.
pub fn context_registration() -> Registration {
    simple_builtin::<ContextBuiltin>()
}

#[cfg(test)]
mod tests {
    use super::*;

    // Canonical (name, scope) table. Must stay in sync with MANIFEST_REGISTRY.
    // Comparison is order-independent: both sides are sorted by name before
    // diffing, so entries can be grouped however makes sense in the registry.
    //
    // When adding a command: add it here AND in MANIFEST_REGISTRY above.
    const EXPECTED: &[(&str, ExecutionScope)] = &[
        // parent-shell
        (".", ExecutionScope::ParentShell),
        ("cd", ExecutionScope::ParentShell),
        ("exec", ExecutionScope::ParentShell),
        ("exit", ExecutionScope::ParentShell),
        ("export", ExecutionScope::ParentShell),
        ("source", ExecutionScope::ParentShell),
        ("unset", ExecutionScope::ParentShell),
        // shell-internal
        ("alias", ExecutionScope::ShellInternal),
        ("bg", ExecutionScope::ShellInternal),
        ("context", ExecutionScope::ShellInternal),
        ("fg", ExecutionScope::ShellInternal),
        ("jobs", ExecutionScope::ShellInternal),
        ("read", ExecutionScope::ShellInternal),
        ("type", ExecutionScope::ShellInternal),
        ("unalias", ExecutionScope::ShellInternal),
        ("wait", ExecutionScope::ShellInternal),
        // subprocess
        ("ask", ExecutionScope::Subprocess),
        ("cat", ExecutionScope::Subprocess),
        ("curl", ExecutionScope::Subprocess),
        ("find", ExecutionScope::Subprocess),
        ("grep", ExecutionScope::Subprocess),
        ("ls", ExecutionScope::Subprocess),
    ];

    #[test]
    fn registry_matches_expected() {
        let mut actual: Vec<(&str, ExecutionScope)> = MANIFEST_REGISTRY
            .iter()
            .map(|m| (m.name, m.scope))
            .collect();
        let mut expected: Vec<(&str, ExecutionScope)> = EXPECTED.to_vec();
        actual.sort_by_key(|e| e.0);
        expected.sort_by_key(|e| e.0);
        assert_eq!(actual, expected);
    }

    #[test]
    fn unknown_command_returns_none() {
        assert_eq!(scope_of("unknown-command"), None);
    }
}
