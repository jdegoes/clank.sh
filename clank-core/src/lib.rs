//! clank-core — shell lifecycle and public API for clank.
//!
//! Wraps [`brush_core`] with clank-specific defaults and exposes a small,
//! stable public API used by `clank-shell`, integration tests, and future
//! embedders.

use std::io::Read as _;

use brush_builtins::{default_builtins, BuiltinSet};
use brush_core::openfiles::{OpenFile, OpenFiles};
pub use brush_core::{CreateOptions, Error, Shell, ShellFd};
use brush_core::{ExecutionParameters, ProcessGroupPolicy};
use brush_parser::{Parser, SourceInfo};
use clank_transcript::TranscriptEntry;

// ---------------------------------------------------------------------------
// Default options
// ---------------------------------------------------------------------------

/// Returns the default [`CreateOptions`] for a non-interactive clank shell.
///
/// These options are used by [`run`] and serve as the canonical baseline for
/// integration tests that need to vary individual fields.
pub fn default_options() -> CreateOptions {
    let mut builtins = default_builtins(BuiltinSet::BashMode);
    builtins.insert("context".to_owned(), clank_builtins::context_registration());
    CreateOptions {
        interactive: false,
        no_profile: true,
        no_rc: true,
        no_editing: true,
        shell_name: Some("clank".to_owned()),
        builtins,
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

/// Boot a shell with the given options and execute `script`, returning the
/// numeric exit code (0–255).
///
/// The script is parsed into individual top-level statements using
/// `brush-parser`. Each statement is executed separately via `run_statement`,
/// producing per-statement `Command` and `Output` transcript entries — the
/// same semantics as `run_interactive`.
pub async fn run_with_options(script: &str, options: CreateOptions) -> Result<u8, Error> {
    let mut shell = Shell::new(options).await?;
    let params = shell.default_exec_params();
    let stmts = parse_statements(script, &shell);
    let mut last_exit_code: u8 = 0;

    for stmt in &stmts {
        let result = run_statement(&mut shell, &params, stmt).await?;
        last_exit_code = result.exit_code.into();
        if result.is_return_or_exit() {
            break;
        }
    }

    Ok(last_exit_code)
}

/// Boot a shell with [`default_options`] and execute `script`.
///
/// Convenience wrapper around [`run_with_options`].
pub async fn run(script: &str) -> Result<u8, Error> {
    run_with_options(script, default_options()).await
}

/// Run an interactive REPL loop against an already-constructed [`Shell`].
///
/// Reads lines from `input`, executes each one against the shell, and
/// continues until EOF or the shell signals an explicit exit. Prints `$ ` to
/// `output` before each line read.
///
/// Returns the exit code of the last executed command, or `0` if no commands
/// were executed before EOF.
pub async fn run_interactive(
    shell: &mut Shell,
    mut input: impl std::io::BufRead,
    mut output: impl std::io::Write,
) -> Result<u8, Error> {
    // Use SameProcessGroup so that external commands (e.g. `ls`) are spawned
    // in the shell's own process group rather than a new one. With
    // NewProcessGroup (the brush default) and interactive: true, brush calls
    // tcsetpgrp to hand terminal foreground to the child — but clank's REPL
    // loop does not perform terminal process-group management, so that call
    // races/hangs. SameProcessGroup is correct for an embedded REPL that does
    // not implement full job control.
    let mut params = shell.default_exec_params();
    params.process_group_policy = ProcessGroupPolicy::SameProcessGroup;
    let mut last_exit_code: u8 = 0;
    let mut line = String::new();

    loop {
        // Emit the prompt. Flush is required because stdout is typically
        // line-buffered and the prompt has no trailing newline.
        write!(output, "$ ").ok();
        output.flush().ok();

        line.clear();
        let bytes_read = input.read_line(&mut line)?;

        if bytes_read == 0 {
            break;
        }

        let cmd = line.trim_end_matches('\n').trim_end_matches('\r');
        if cmd.is_empty() {
            continue;
        }

        let result = run_statement(shell, &params, cmd).await?;
        last_exit_code = result.exit_code.into();

        if result.is_return_or_exit() {
            break;
        }
    }

    Ok(last_exit_code)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Execute a single shell statement: capture its stdout, record it in the
/// transcript, echo the captured output to the real terminal stdout, and
/// return the execution result.
///
/// Recording happens *after* execution so the command is not visible to itself
/// (e.g. `context show` sees the transcript as it was before it ran).
///
/// Output from `context show` and `context summarize` is suppressed from the
/// transcript to prevent the transcript from duplicating itself on inspection.
async fn run_statement(
    shell: &mut Shell,
    params: &ExecutionParameters,
    cmd: &str,
) -> Result<brush_core::ExecutionResult, Error> {
    // Temporarily replace the shell's stdout with a pipe write end so we can
    // capture command output. stdin and stderr are preserved — replace_open_files
    // replaces the entire FD table so all three descriptors must be supplied.
    let (mut cap_reader, cap_writer) =
        std::io::pipe().map_err(|e| Error::from(brush_core::ErrorKind::from(e)))?;
    shell.replace_open_files(
        [
            (OpenFiles::STDIN_FD, OpenFile::Stdin(std::io::stdin())),
            (OpenFiles::STDOUT_FD, OpenFile::PipeWriter(cap_writer)),
            (OpenFiles::STDERR_FD, OpenFile::Stderr(std::io::stderr())),
        ]
        .into_iter(),
    );

    let result = shell.run_string(cmd, params).await?;

    // Restore all standard FDs, then drain the captured output.
    shell.replace_open_files(
        [
            (OpenFiles::STDIN_FD, OpenFile::Stdin(std::io::stdin())),
            (OpenFiles::STDOUT_FD, OpenFile::Stdout(std::io::stdout())),
            (OpenFiles::STDERR_FD, OpenFile::Stderr(std::io::stderr())),
        ]
        .into_iter(),
    );
    let mut captured = String::new();
    cap_reader.read_to_string(&mut captured).ok();

    // Echo captured output to the real terminal so the user still sees it.
    if !captured.is_empty() {
        print!("{captured}");
    }

    // context clear and context trim are transcript-mutating commands that must
    // not record themselves: after context clear the transcript must be empty;
    // after context trim the trimmed state must be exact. Recording them would
    // leave a residual entry that undermines the operation's intent.
    //
    // context show and context summarize must not record their output back, but
    // their invocation is still recorded as a command entry.
    if !is_self_erasing_command(cmd) {
        clank_transcript::global()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(TranscriptEntry::command(cmd));

        if !captured.is_empty() && !is_inspection_command(cmd) {
            clank_transcript::global()
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push(TranscriptEntry::output(captured.trim_end_matches('\n')));
        }
    }

    Ok(result)
}

/// Parse `source` into a list of top-level statement source texts using
/// `brush-parser`.
///
/// Uses the shell's parser options so the pre-parse is consistent with how
/// brush-core itself would parse the same script. Returns one string per
/// `CompleteCommand` AST node; multi-line constructs (`if/fi`, `for/done`,
/// function definitions, etc.) produce a single entry.
///
/// If parsing fails, the entire `source` is returned as a single entry so
/// execution can still proceed — brush-core will produce the definitive parse
/// error at run time via `run_string`.
///
/// An empty script returns an empty `Vec` — no iterations, exit code 0.
fn parse_statements(source: &str, shell: &Shell) -> Vec<String> {
    let reader = std::io::Cursor::new(source);
    let source_info = SourceInfo {
        source: "script".to_owned(),
    };
    let mut parser = Parser::new(reader, &shell.parser_options(), &source_info);
    let program = match parser.parse_program() {
        Ok(p) => p,
        Err(_) => return vec![source.to_owned()],
    };

    // Split source into lines for verbatim text extraction.
    // brush-parser's character-level source location tracking is incomplete
    // (CommandPrefixOrSuffixItem has a TODO and returns None for assignments
    // and redirects, causing SimpleCommand.location() to cover only the
    // command word). Using line numbers sidesteps this: start.line is always
    // accurate, and for single-line statements (the vast majority) the full
    // source line is the correct statement text. Multi-line compound constructs
    // (if/fi, for/done, function bodies) have correct outermost locations.
    //
    // Known limitation: two statements on one line separated by `;` both map
    // to the same line range and would be recorded as one entry. Acceptable
    // for v1; the fix requires upstream brush-parser completing its TODO.
    let lines: Vec<&str> = source.lines().collect();

    let stmts = program
        .complete_commands
        .iter()
        .map(|cmd| {
            use brush_parser::ast::SourceLocation as _;
            cmd.location()
                .and_then(|loc| {
                    // line numbers are 1-based; end is the line of the last token.
                    let start_line = loc.start.line.saturating_sub(1); // to 0-based
                    let end_line = loc.end.line; // exclusive upper (0-based)
                    let end_line = end_line.min(lines.len());
                    if start_line >= end_line {
                        return None;
                    }
                    let text = lines[start_line..end_line].join("\n").trim().to_owned();
                    if text.is_empty() {
                        None
                    } else {
                        Some(text)
                    }
                })
                // Fallback: Display when location unavailable (e.g. function
                // definitions without redirect lists).
                .unwrap_or_else(|| format!("{cmd}").trim().to_owned())
        })
        .filter(|s| !s.is_empty())
        .collect();

    stmts
}

/// Returns `true` if `cmd` is a transcript-mutating command that must not
/// record itself.
///
/// `context clear` — after it runs the transcript must be empty; recording it
/// would leave a residual entry. The README states: "discard transcript (AI
/// starts fresh on next ask)".
///
/// `context trim <n>` — the trimmed state must be exact; a self-entry would
/// corrupt the count.
fn is_self_erasing_command(cmd: &str) -> bool {
    let trimmed = cmd.trim();
    trimmed == "context clear"
        || trimmed.starts_with("context clear ")
        || trimmed == "context trim"
        || trimmed.starts_with("context trim ")
}

/// Returns `true` if `cmd` is a transcript-inspection command whose output
/// must not be recorded back into the transcript.
///
/// Per the README: "`context show` and `context summarize` are
/// transcript-inspection commands: their output is written to stdout but is
/// not recorded back into the transcript."
fn is_inspection_command(cmd: &str) -> bool {
    let trimmed = cmd.trim();
    trimmed == "context show"
        || trimmed.starts_with("context show ")
        || trimmed == "context summarize"
        || trimmed.starts_with("context summarize ")
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

    // --- parse_statements ---

    #[tokio::test]
    async fn parse_statements_function_def_is_one_statement() {
        let shell = Shell::new(default_options()).await.unwrap();
        let source = "f() { local v=inner; echo $v; }\nf\necho ${v:-outer}";
        let stmts = parse_statements(source, &shell);
        assert_eq!(stmts.len(), 3, "expected 3 statements: got {stmts:?}");
        // Function definitions have no location in brush-parser (FunctionBody.location()
        // returns None without a redirect list), so Display fallback is used. The
        // Display form is valid shell even if it differs from the original.
        assert!(
            stmts[0].contains("local v=inner") && stmts[0].contains("echo $v"),
            "function body should be present: {}",
            stmts[0]
        );
        assert_eq!(stmts[1], "f");
        assert_eq!(stmts[2], "echo ${v:-outer}");
    }

    #[tokio::test]
    async fn parse_statements_if_block_is_one_statement() {
        let shell = Shell::new(default_options()).await.unwrap();
        let source = "if true; then\n  echo yes\nfi";
        let stmts = parse_statements(source, &shell);
        assert_eq!(
            stmts.len(),
            1,
            "if/fi should be one statement: got {stmts:?}"
        );
    }

    #[tokio::test]
    async fn parse_statements_export_is_correct() {
        let shell = Shell::new(default_options()).await.unwrap();
        let source = "export FOO=hello\necho $FOO";
        let stmts = parse_statements(source, &shell);
        assert_eq!(stmts.len(), 2, "expected 2 statements: got {stmts:?}");
        assert_eq!(
            stmts[0], "export FOO=hello",
            "unexpected export stmt: {:?}",
            stmts[0]
        );
        assert_eq!(stmts[1], "echo $FOO");
    }

    #[tokio::test]
    async fn parse_statements_empty_is_empty() {
        let shell = Shell::new(default_options()).await.unwrap();
        let stmts = parse_statements("", &shell);
        assert!(stmts.is_empty());
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
