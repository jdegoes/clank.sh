//! clank-core — shell lifecycle and public API for clank.
//!
//! Wraps [`brush_core`] with clank-specific defaults and exposes a small,
//! stable public API used by `clank-shell`, integration tests, and future
//! embedders.

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

/// Returns [`CreateOptions`] for an interactive clank shell session.
///
/// Identical to [`default_options`] except `interactive` is `true`, which
/// signals to `brush-core` that the shell is running in an interactive
/// terminal context.
pub fn interactive_options() -> CreateOptions {
    CreateOptions {
        interactive: true,
        ..default_options()
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

/// Run an interactive REPL loop against an already-constructed [`Shell`].
///
/// Reads lines from `input`, executes each one against the shell, and
/// continues until EOF or the shell signals an explicit exit (e.g. the user
/// types `exit`). Prints `$ ` to `output` before each line read.
///
/// Returns the exit code of the last executed command, or `0` if no commands
/// were executed before EOF.
///
/// # Why take `input` and `output`?
///
/// Accepting `impl BufRead` and `impl Write` instead of hardcoding
/// `stdin`/`stdout` lets integration tests drive the loop with in-memory
/// buffers without spawning a subprocess.
pub async fn run_interactive(
    shell: &mut Shell,
    mut input: impl std::io::BufRead,
    mut output: impl std::io::Write,
) -> Result<u8, Error> {
    let params = shell.default_exec_params();
    let mut last_exit_code: u8 = 0;
    let mut line = String::new();

    loop {
        // Emit the prompt. Flush is required because stdout is typically
        // line-buffered and the prompt has no trailing newline.
        write!(output, "$ ").ok();
        output.flush().ok();

        line.clear();
        let bytes_read = input.read_line(&mut line)?;

        // EOF — clean termination.
        if bytes_read == 0 {
            break;
        }

        // Strip the trailing newline; skip truly empty lines.
        let cmd = line.trim_end_matches('\n').trim_end_matches('\r');
        if cmd.is_empty() {
            continue;
        }

        let result = shell.run_string(cmd, &params).await?;
        last_exit_code = result.exit_code.into();

        // `exit [n]` inside the script sets ExitShell control flow.
        if result.is_return_or_exit() {
            break;
        }
    }

    Ok(last_exit_code)
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

    // --- interactive_options field coverage ---

    #[test]
    fn interactive_options_is_interactive() {
        assert!(interactive_options().interactive);
    }

    #[test]
    fn interactive_options_skips_profile() {
        assert!(interactive_options().no_profile);
    }

    #[test]
    fn interactive_options_skips_rc() {
        assert!(interactive_options().no_rc);
    }

    #[test]
    fn interactive_options_disables_editing() {
        assert!(interactive_options().no_editing);
    }

    #[test]
    fn interactive_options_shell_name_is_clank() {
        assert_eq!(interactive_options().shell_name, Some("clank".to_owned()));
    }

    // --- run_interactive() behaviour ---
    //
    // These tests verify exit-code and control-flow semantics. Command output
    // goes to the shell's inherited stdout (not the `output` writer), so
    // output content is covered by the acceptance test suite instead.

    #[tokio::test]
    async fn run_interactive_exits_zero_after_successful_commands() {
        let input = b"true\ntrue\n" as &[u8];
        let mut shell = Shell::new(interactive_options())
            .await
            .expect("shell creation should not error");
        let code = run_interactive(&mut shell, input, std::io::sink())
            .await
            .expect("run_interactive should not error");
        assert_eq!(code, 0);
    }

    #[tokio::test]
    async fn run_interactive_propagates_exit_code_from_exit_command() {
        let input = b"exit 7\n" as &[u8];
        let mut shell = Shell::new(interactive_options())
            .await
            .expect("shell creation should not error");
        let code = run_interactive(&mut shell, input, std::io::sink())
            .await
            .expect("run_interactive should not error");
        assert_eq!(code, 7, "exit code should be 7");
    }

    #[tokio::test]
    async fn run_interactive_stops_after_exit_command() {
        // Commands after `exit` must not execute. We verify this by checking
        // that the exit code matches the `exit` argument, not any later command.
        let input = b"exit 3\nexit 99\n" as &[u8];
        let mut shell = Shell::new(interactive_options())
            .await
            .expect("shell creation should not error");
        let code = run_interactive(&mut shell, input, std::io::sink())
            .await
            .expect("run_interactive should not error");
        assert_eq!(code, 3, "loop should stop at the first exit");
    }

    #[tokio::test]
    async fn run_interactive_returns_zero_on_eof_with_no_commands() {
        let input = b"" as &[u8];
        let mut shell = Shell::new(interactive_options())
            .await
            .expect("shell creation should not error");
        let code = run_interactive(&mut shell, input, std::io::sink())
            .await
            .expect("run_interactive should not error");
        assert_eq!(code, 0, "empty input should exit 0");
    }

    #[tokio::test]
    async fn run_interactive_returns_last_exit_code_on_eof() {
        let input = b"true\nfalse\n" as &[u8];
        let mut shell = Shell::new(interactive_options())
            .await
            .expect("shell creation should not error");
        let code = run_interactive(&mut shell, input, std::io::sink())
            .await
            .expect("run_interactive should not error");
        assert_ne!(
            code, 0,
            "last command was false; exit code should be nonzero"
        );
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
