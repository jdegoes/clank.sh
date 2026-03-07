mod echo;
mod false_cmd;
mod ls;
mod true_cmd;

use brush_core::{Shell, builtins::builtin};

/// Register all clank builtin commands on the shell.
///
/// Overrides any brush-builtins defaults for the same command names.
/// Called once during shell construction in `clank::build_shell()`.
pub fn register(shell: &mut Shell) {
    shell.register_builtin("echo", builtin::<echo::EchoCommand>());
    shell.register_builtin("false", builtin::<false_cmd::FalseCommand>());
    shell.register_builtin("ls", builtin::<ls::LsCommand>());
    shell.register_builtin("true", builtin::<true_cmd::TrueCommand>());
}

#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn register_succeeds() {
        // build_shell() calls register() internally — if it doesn't panic, registration succeeded.
        let _shell = clank::build_shell().await;
    }

    #[tokio::test]
    async fn echo_runs_internally() {
        let mut shell = clank::build_shell().await;
        let outcome = shell.run_command("echo hello").await;
        assert_eq!(outcome.exit_code, 0, "echo should succeed");
    }

    #[tokio::test]
    async fn true_exits_zero() {
        let mut shell = clank::build_shell().await;
        let outcome = shell.run_command("true").await;
        assert_eq!(outcome.exit_code, 0);
    }

    #[tokio::test]
    async fn false_exits_one() {
        let mut shell = clank::build_shell().await;
        let outcome = shell.run_command("false").await;
        assert_eq!(outcome.exit_code, 1);
    }
}
