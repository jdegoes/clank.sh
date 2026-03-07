use std::io::{self, BufRead, Write};

use brush_builtins::{BuiltinSet, ShellBuilderExt};
use brush_core::Shell;

/// Build a new clank shell instance with BashMode builtins.
/// Skips profile and rc sourcing — clank.sh manages its own init.
pub async fn build_shell() -> Shell {
    Shell::builder()
        .default_builtins(BuiltinSet::BashMode)
        .shell_name("clank".to_string())
        .no_profile(true)
        .no_rc(true)
        .build()
        .await
        .expect("failed to create shell")
}

/// Run a read-eval-print loop over stdin until EOF or `exit`.
///
/// The prompt is written to stderr so it does not pollute stdout
/// (important for test assertions and future pipe use).
pub async fn run_repl(mut shell: Shell) {
    let stdin = io::stdin();
    let mut lines = stdin.lock().lines();

    loop {
        eprint!("$ ");
        let _ = io::stderr().flush();

        match lines.next() {
            None => break, // EOF / Ctrl-D
            Some(Err(e)) => {
                eprintln!("clank: read error: {e}");
                break;
            }
            Some(Ok(line)) => {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                if trimmed == "exit" {
                    break;
                }
                let params = shell.default_exec_params();
                if let Err(e) = shell.run_string(trimmed, &params).await {
                    eprintln!("clank: {e}");
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// build_shell() must succeed and return a usable Shell.
    #[tokio::test]
    async fn build_shell_succeeds() {
        let _shell = build_shell().await;
        // If we reach here without panic, shell construction succeeded.
    }

    /// A freshly built shell can execute a trivial command without error.
    #[tokio::test]
    async fn shell_runs_true() {
        let mut shell = build_shell().await;
        let params = shell.default_exec_params();
        let result = shell.run_string("true", &params).await;
        assert!(result.is_ok(), "expected `true` to succeed but got an error");
    }

    /// A freshly built shell reports exit code 0 after a successful command.
    #[tokio::test]
    async fn shell_exit_code_zero_after_success() {
        let mut shell = build_shell().await;
        let params = shell.default_exec_params();
        shell.run_string("true", &params).await.unwrap();
        assert_eq!(shell.last_result(), 0);
    }

    /// A freshly built shell reports a non-zero exit code after a failing command.
    #[tokio::test]
    async fn shell_exit_code_nonzero_after_failure() {
        let mut shell = build_shell().await;
        let params = shell.default_exec_params();
        // `false` exits with code 1 — run_string itself succeeds (no Err),
        // but the shell's last result is non-zero.
        let _ = shell.run_string("false", &params).await;
        assert_ne!(shell.last_result(), 0);
    }

    /// Shell name is set correctly.
    #[tokio::test]
    async fn shell_name_is_clank() {
        let mut shell = build_shell().await;
        let params = shell.default_exec_params();
        // $0 should be "clank"
        shell.run_string("echo $0", &params).await.unwrap();
        // We can't easily capture stdout in a unit test — this just ensures
        // the command doesn't error. Full stdout assertion is in integration tests.
    }
}
