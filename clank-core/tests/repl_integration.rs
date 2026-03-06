//! Integration tests for `clank_core::Repl`.
//!
//! These tests drive the REPL through its public API using in-memory I/O,
//! verifying that commands execute and produce the expected exit behaviour.
//!
//! Note: command output (stdout from brush-core builtins like `echo`) goes
//! through the process's real stdout, not through `prompt_out`. These tests
//! therefore focus on REPL control-flow (no panic, correct return) rather
//! than capturing command output — that is covered by the binary-level
//! acceptance tests in `clank-shell/tests/acceptance.rs`.

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use clank_core::Repl;
    use std::io::Cursor;

    /// Helper: run the REPL with the given script, discarding prompt output.
    async fn run_script(script: &str) -> anyhow::Result<()> {
        let mut repl = Repl::new().await?;
        let input = Cursor::new(script.to_string());
        let mut prompt_out = Vec::<u8>::new();
        repl.run(input, &mut prompt_out).await
    }

    #[tokio::test]
    async fn empty_input_returns_ok() {
        run_script("").await.expect("empty input should return Ok");
    }

    #[tokio::test]
    async fn single_comment_returns_ok() {
        run_script("# this is a comment\n")
            .await
            .expect("comment-only input should return Ok");
    }

    #[tokio::test]
    async fn blank_lines_are_skipped() {
        run_script("\n\n\n")
            .await
            .expect("blank lines should return Ok");
    }

    #[tokio::test]
    async fn prompt_is_written_for_each_line() {
        let mut repl = Repl::new().await.expect("Repl::new");
        let input = Cursor::new("true\ntrue\n");
        let mut prompt_out = Vec::<u8>::new();
        repl.run(input, &mut prompt_out)
            .await
            .expect("run should succeed");
        let output = String::from_utf8(prompt_out).expect("prompt output is utf8");
        // One "$ " per non-empty line.
        assert_eq!(output.matches("$ ").count(), 2, "expected 2 prompts");
    }

    #[tokio::test]
    async fn variable_assignment_and_expansion() {
        // If brush-core handles $VAR expansion correctly this won't error.
        run_script("X=hello\necho $X\n")
            .await
            .expect("variable assignment and expansion should succeed");
    }

    #[tokio::test]
    async fn pipeline_executes() {
        run_script("echo hello | cat\n")
            .await
            .expect("pipeline should execute without error");
    }
}
