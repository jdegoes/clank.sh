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
    use super::*;

    #[tokio::test]
    async fn register_succeeds() {
        let mut shell = clank::build_shell().await;
        // Should not panic — registration of all three commands completes cleanly.
        register(&mut shell);
    }

    #[tokio::test]
    async fn echo_runs_internally() {
        let mut shell = clank::build_shell().await;
        register(&mut shell);
        let params = shell.default_exec_params();
        let result = shell.run_string("echo hello", &params).await;
        assert!(result.is_ok(), "echo should succeed");
        assert_eq!(shell.last_result(), 0);
    }

    #[tokio::test]
    async fn true_exits_zero() {
        let mut shell = clank::build_shell().await;
        register(&mut shell);
        let params = shell.default_exec_params();
        shell.run_string("true", &params).await.unwrap();
        assert_eq!(shell.last_result(), 0);
    }

    #[tokio::test]
    async fn false_exits_one() {
        let mut shell = clank::build_shell().await;
        register(&mut shell);
        let params = shell.default_exec_params();
        let _ = shell.run_string("false", &params).await;
        assert_eq!(shell.last_result(), 1);
    }
}
