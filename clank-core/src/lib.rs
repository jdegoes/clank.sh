//! clank-core — the reusable shell logic for clank.
//!
//! This crate is the library heart of clank. It wraps [`brush_core`] with
//! clank-specific defaults and exposes a small, stable public API used by:
//!
//! - `clank-shell` (the binary entry point)
//! - `clank-core/tests/` (integration tests)
//! - Future embedders and tooling
//!
//! All direct `brush_core` imports live here. No other crate in the workspace
//! depends on `brush_core` directly.

use brush_builtins::{default_builtins, BuiltinSet};
pub use brush_core::{CreateOptions, Error, Shell};

// ---------------------------------------------------------------------------
// Default options
// ---------------------------------------------------------------------------

/// Returns the default [`CreateOptions`] for a non-interactive clank shell.
///
/// These options are used by [`run`] and serve as the canonical baseline for
/// integration tests that need to vary individual fields.
pub fn default_options() -> CreateOptions {
    CreateOptions {
        interactive: false,
        no_profile: true,
        no_rc: true,
        no_editing: true,
        shell_name: Some("clank".to_owned()),
        builtins: default_builtins(BuiltinSet::BashMode),
        ..CreateOptions::default()
    }
}

// ---------------------------------------------------------------------------
// Execution helpers
// ---------------------------------------------------------------------------

/// Boot a shell with the given options and execute `command`, returning the
/// numeric exit code (0–255).
pub async fn run_with_options(command: &str, options: CreateOptions) -> Result<u8, Error> {
    let mut shell = Shell::new(options).await?;
    let params = shell.default_exec_params();
    let result = shell.run_string(command, &params).await?;
    Ok(result.exit_code.into())
}

/// Boot a shell with [`default_options`] and execute `command`.
///
/// Convenience wrapper around [`run_with_options`].
pub async fn run(command: &str) -> Result<u8, Error> {
    run_with_options(command, default_options()).await
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // --- default_options field coverage ---

    #[test]
    fn default_options_is_non_interactive() {
        assert!(!default_options().interactive);
    }

    #[test]
    fn default_options_skips_profile() {
        assert!(default_options().no_profile);
    }

    #[test]
    fn default_options_skips_rc() {
        assert!(default_options().no_rc);
    }

    #[test]
    fn default_options_disables_editing() {
        assert!(default_options().no_editing);
    }

    #[test]
    fn default_options_shell_name_is_clank() {
        assert_eq!(default_options().shell_name, Some("clank".to_owned()));
    }

    // --- run() behaviour ---

    #[tokio::test]
    async fn echo_hello_exits_zero() {
        let code = run("echo hello").await.expect("shell should not error");
        assert_eq!(code, 0, "echo hello should exit 0");
    }

    #[tokio::test]
    async fn false_exits_nonzero() {
        let code = run("false").await.expect("shell should not error");
        assert_ne!(code, 0, "false should exit non-zero");
    }
}
