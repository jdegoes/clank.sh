//! clank-builtins — execution scope metadata and clank-owned builtin commands.
//!
//! This crate owns the `ExecutionScope` classification, the `CommandManifest`
//! type, the static manifest registry for all commands clank recognises, and
//! the implementations of clank-owned shell-internal builtins.

use std::io::Write as _;

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
/// Carries the command name and its execution scope. Additional fields
/// (authorization policy, input/output schema, help text, subcommands) are
/// added in future work.
pub struct CommandManifest {
    pub name: &'static str,
    pub scope: ExecutionScope,
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
    },
    CommandManifest {
        name: "cd",
        scope: ExecutionScope::ParentShell,
    },
    CommandManifest {
        name: "exec",
        scope: ExecutionScope::ParentShell,
    },
    CommandManifest {
        name: "exit",
        scope: ExecutionScope::ParentShell,
    },
    CommandManifest {
        name: "export",
        scope: ExecutionScope::ParentShell,
    },
    CommandManifest {
        name: "source",
        scope: ExecutionScope::ParentShell,
    },
    CommandManifest {
        name: "unset",
        scope: ExecutionScope::ParentShell,
    },
    // shell-internal: operate on shell-owned tables
    CommandManifest {
        name: "alias",
        scope: ExecutionScope::ShellInternal,
    },
    CommandManifest {
        name: "bg",
        scope: ExecutionScope::ShellInternal,
    },
    CommandManifest {
        name: "context",
        scope: ExecutionScope::ShellInternal,
    },
    CommandManifest {
        name: "fg",
        scope: ExecutionScope::ShellInternal,
    },
    CommandManifest {
        name: "history",
        scope: ExecutionScope::ShellInternal,
    },
    CommandManifest {
        name: "jobs",
        scope: ExecutionScope::ShellInternal,
    },
    CommandManifest {
        name: "read",
        scope: ExecutionScope::ShellInternal,
    },
    CommandManifest {
        name: "type",
        scope: ExecutionScope::ShellInternal,
    },
    CommandManifest {
        name: "unalias",
        scope: ExecutionScope::ShellInternal,
    },
    CommandManifest {
        name: "wait",
        scope: ExecutionScope::ShellInternal,
    },
    // subprocess: isolated execution
    CommandManifest {
        name: "ask",
        scope: ExecutionScope::Subprocess,
    },
    CommandManifest {
        name: "cat",
        scope: ExecutionScope::Subprocess,
    },
    CommandManifest {
        name: "curl",
        scope: ExecutionScope::Subprocess,
    },
    CommandManifest {
        name: "find",
        scope: ExecutionScope::Subprocess,
    },
    CommandManifest {
        name: "grep",
        scope: ExecutionScope::Subprocess,
    },
    CommandManifest {
        name: "ls",
        scope: ExecutionScope::Subprocess,
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

// ---------------------------------------------------------------------------
// context builtin
// ---------------------------------------------------------------------------

/// Implementation of the `context` shell-internal builtin.
///
/// Subcommands: `show`, `clear`, `trim <n>`.
pub struct ContextBuiltin;

impl SimpleCommand for ContextBuiltin {
    fn get_content(name: &str, content_type: ContentType) -> Result<String, brush_core::Error> {
        let usage = format!("usage: {name} show|clear|trim <n>\n");
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
            "show" => {
                let transcript = clank_transcript::global();
                let locked = transcript.lock().unwrap();
                let mut stdout = context.stdout();
                for entry in locked.entries() {
                    writeln!(stdout, "{entry}").ok();
                }
                Ok(ExecutionResult::success())
            }
            "clear" => {
                clank_transcript::global().lock().unwrap().clear();
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
                    "context: unknown subcommand {other:?}: expected show, clear, or trim"
                )
                .ok();
                Ok(ExecutionResult::from(
                    brush_core::results::ExecutionExitCode::InvalidUsage,
                ))
            }
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
        ("history", ExecutionScope::ShellInternal),
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
