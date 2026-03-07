use std::collections::HashMap;

/// Execution scope of a command, as defined in the clank spec.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutionScope {
    /// Runs in the parent shell context; mutates shell state. E.g. `cd`, `export`.
    ParentShell,
    /// Implemented in the shell; operates on shell-internal tables. E.g. `jobs`, `context`.
    ShellInternal,
    /// Runs as a subprocess; no access to parent shell state. E.g. `ls`, `ask`.
    Subprocess,
}

/// Authorization policy for a command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthorizationPolicy {
    Allow,
    Confirm,
    SudoOnly,
}

/// The manifest for a single resolvable command.
#[derive(Debug, Clone)]
pub struct CommandManifest {
    pub name: String,
    pub synopsis: String,
    pub execution_scope: ExecutionScope,
    pub authorization_policy: AuthorizationPolicy,
    pub help_text: String,
    pub subcommands: Vec<CommandManifest>,
    /// If true, this command is included in the `ask` tool surface regardless
    /// of its `execution_scope`. Used for `prompt-user`, which is
    /// `ShellInternal` but is explicitly exposed to the model per the spec.
    pub expose_to_model: bool,
}

impl CommandManifest {
    /// Create a simple manifest with default allow policy.
    pub fn simple(
        name: impl Into<String>,
        synopsis: impl Into<String>,
        scope: ExecutionScope,
    ) -> Self {
        Self {
            name: name.into(),
            synopsis: synopsis.into(),
            execution_scope: scope,
            authorization_policy: AuthorizationPolicy::Allow,
            help_text: String::new(),
            subcommands: Vec::new(),
            expose_to_model: false,
        }
    }
}

/// A registry of command manifests for all installed commands.
///
/// Used by `ask` to enumerate the tool surface, by tab completion, and by
/// the authorization model. All registrations happen at shell startup or
/// on `grease install`.
#[derive(Debug, Default)]
pub struct ManifestRegistry {
    manifests: HashMap<String, CommandManifest>,
}

impl ManifestRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a manifest, replacing any existing entry for the same name.
    pub fn register(&mut self, manifest: CommandManifest) {
        self.manifests.insert(manifest.name.clone(), manifest);
    }

    /// Look up a manifest by command name.
    pub fn get(&self, name: &str) -> Option<&CommandManifest> {
        self.manifests.get(name)
    }

    /// Return all manifests in the `ask` tool surface: all `Subprocess`-scoped
    /// commands plus any with `expose_to_model = true` (notably `prompt-user`).
    ///
    /// Per spec: "The exception is `prompt-user`: although `shell-internal`, it
    /// is explicitly exposed to the model as a tool because it is the mechanism
    /// by which the model communicates back to the human during a task."
    pub fn subprocess_commands(&self) -> Vec<&CommandManifest> {
        let mut cmds: Vec<_> = self
            .manifests
            .values()
            .filter(|m| m.execution_scope == ExecutionScope::Subprocess || m.expose_to_model)
            .collect();
        cmds.sort_by(|a, b| a.name.cmp(&b.name));
        cmds
    }

    /// Populate the registry with the initial set of commands known at
    /// shell startup. Called once by `ClankShell::new` (via the binary).
    pub fn populate_defaults(&mut self) {
        use ExecutionScope::*;

        // parent-shell builtins
        for name in &["cd", "exec", "exit", "export", "source", "unset"] {
            self.register(CommandManifest::simple(*name, "", ParentShell));
        }

        // shell-internal builtins
        for name in &[
            "alias", "context", "fg", "bg", "history", "jobs", "read", "type", "wait", "which",
        ] {
            self.register(CommandManifest::simple(*name, "", ShellInternal));
        }

        // prompt-user is ShellInternal but explicitly exposed to the model per spec.
        self.register(CommandManifest {
            expose_to_model: true,
            ..CommandManifest::simple(
                "prompt-user",
                "Pause and present a question to the human user",
                ShellInternal,
            )
        });

        // Core subprocess commands — read-only: allow
        for name in &[
            "ls", "cat", "find", "grep", "sed", "awk", "sort", "uniq", "wc", "head", "tail", "cut",
            "tr", "xargs", "diff", "printf", "test", "echo", "sleep", "stat", "file", "jq", "env",
            "ps", "man",
        ] {
            self.register(CommandManifest::simple(*name, "", Subprocess));
        }

        // Outbound HTTP — confirm
        for name in &["curl", "wget"] {
            self.register(CommandManifest {
                authorization_policy: AuthorizationPolicy::Confirm,
                ..CommandManifest::simple(*name, "", Subprocess)
            });
        }

        // Write to home / destructive — confirm
        for name in &["cp", "mv", "mkdir", "touch", "tee", "patch"] {
            self.register(CommandManifest {
                authorization_policy: AuthorizationPolicy::Confirm,
                ..CommandManifest::simple(*name, "", Subprocess)
            });
        }

        // Destructive — sudo-only
        for name in &["rm", "kill"] {
            self.register(CommandManifest {
                authorization_policy: AuthorizationPolicy::SudoOnly,
                ..CommandManifest::simple(*name, "", Subprocess)
            });
        }

        // AI / platform commands
        for name in &["ask", "model", "mcp", "golem", "grease"] {
            self.register(CommandManifest::simple(*name, "", Subprocess));
        }
    }
}

// ---------------------------------------------------------------------------
// Global registry
// ---------------------------------------------------------------------------

use std::sync::{LazyLock, RwLock};

/// Global manifest registry, populated at shell startup via `populate_defaults()`
/// and updated by `grease install/remove`. All crates that need to query the
/// command manifest surface use this singleton.
pub static GLOBAL_REGISTRY: LazyLock<RwLock<ManifestRegistry>> =
    LazyLock::new(|| RwLock::new(ManifestRegistry::new()));

static REGISTRY_INIT: std::sync::OnceLock<()> = std::sync::OnceLock::new();

/// Populate the global registry with defaults.
///
/// Idempotent: the first call populates the registry; subsequent calls are
/// no-ops. This allows `ClankShell::new()` to call this on every construction
/// without redundant re-registration.
pub fn init_global_registry() {
    REGISTRY_INIT.get_or_init(|| {
        GLOBAL_REGISTRY
            .write()
            .expect("manifest registry poisoned")
            .populate_defaults();
    });
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn registry_with_defaults() -> ManifestRegistry {
        let mut r = ManifestRegistry::new();
        r.populate_defaults();
        r
    }

    #[test]
    fn test_manifest_registry_lookup() {
        let r = registry_with_defaults();
        assert!(r.get("ask").is_some());
        assert!(r.get("ls").is_some());
        assert!(r.get("cd").is_some());
        assert!(r.get("nonexistent").is_none());
    }

    #[test]
    fn test_manifest_registry_subprocess_filter() {
        let r = registry_with_defaults();
        let subprocess = r.subprocess_commands();
        let names: Vec<_> = subprocess.iter().map(|m| m.name.as_str()).collect();
        // subprocess-scoped commands are included
        assert!(names.contains(&"ask"));
        assert!(names.contains(&"ls"));
        // context and cd are NOT in the tool surface
        assert!(!names.contains(&"context"));
        assert!(!names.contains(&"cd"));
    }

    #[test]
    fn test_prompt_user_in_tool_surface() {
        // prompt-user is ShellInternal but must be in the tool surface per spec.
        let r = registry_with_defaults();
        let names: Vec<_> = r
            .subprocess_commands()
            .iter()
            .map(|m| m.name.as_str())
            .collect();
        assert!(
            names.contains(&"prompt-user"),
            "prompt-user must be in the ask tool surface"
        );
        // Verify it's still registered as ShellInternal scope.
        let m = r.get("prompt-user").unwrap();
        assert_eq!(m.execution_scope, ExecutionScope::ShellInternal);
        assert!(m.expose_to_model);
        // context should not be in the tool surface
        assert!(!names.contains(&"context"));
    }

    #[test]
    fn test_manifest_registry_register_replaces() {
        let mut r = ManifestRegistry::new();
        r.register(CommandManifest::simple(
            "foo",
            "first",
            ExecutionScope::Subprocess,
        ));
        r.register(CommandManifest::simple(
            "foo",
            "second",
            ExecutionScope::Subprocess,
        ));
        assert_eq!(r.get("foo").unwrap().synopsis, "second");
    }

    #[test]
    fn test_manifest_sudo_only_commands() {
        // These commands are the last line of defence before destructive operations.
        // If any of them were accidentally registered as Allow, the authorization
        // check would stop protecting them.
        let r = registry_with_defaults();
        for name in &["rm", "kill"] {
            let policy = &r
                .get(name)
                .unwrap_or_else(|| panic!("{name} must be registered"))
                .authorization_policy;
            assert_eq!(
                *policy,
                AuthorizationPolicy::SudoOnly,
                "{name} must be SudoOnly"
            );
        }
    }

    #[test]
    fn test_manifest_confirm_commands() {
        let r = registry_with_defaults();
        for name in &["curl", "wget", "cp", "mv", "mkdir", "touch", "tee", "patch"] {
            let policy = &r
                .get(name)
                .unwrap_or_else(|| panic!("{name} must be registered"))
                .authorization_policy;
            assert_eq!(
                *policy,
                AuthorizationPolicy::Confirm,
                "{name} must be Confirm"
            );
        }
    }

    #[test]
    fn test_manifest_allow_commands_are_not_elevated() {
        // Read-only commands must remain Allow — elevating them would require
        // sudo to run basic shell inspection tools.
        let r = registry_with_defaults();
        for name in &["ls", "cat", "grep", "ps", "stat", "echo", "find"] {
            let policy = &r
                .get(name)
                .unwrap_or_else(|| panic!("{name} must be registered"))
                .authorization_policy;
            assert_eq!(
                *policy,
                AuthorizationPolicy::Allow,
                "{name} must be Allow (not elevated)"
            );
        }
    }

    #[test]
    fn test_manifest_registry_subprocess_sorted() {
        let r = registry_with_defaults();
        let subprocess = r.subprocess_commands();
        let names: Vec<_> = subprocess.iter().map(|m| m.name.as_str()).collect();
        let mut sorted = names.clone();
        sorted.sort();
        assert_eq!(names, sorted, "subprocess_commands should be sorted");
    }
}
