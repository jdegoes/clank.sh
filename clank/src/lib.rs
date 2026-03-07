mod transcript;

use std::io::{self, BufRead, Read, Write};

use brush_builtins::{BuiltinSet, ShellBuilderExt};
use brush_core::openfiles::OpenFile;
use brush_core::Shell;

pub use transcript::{CommandOutcome, Transcript, TranscriptEntry};

// ── ClankShell ────────────────────────────────────────────────────────────────

/// The primary shell abstraction for clank.sh.
///
/// Wraps `brush_core::Shell` (the bash-compatible interpreter) with a
/// `Transcript` — the session memory that `ask` reads as its context window.
pub struct ClankShell {
    /// The underlying bash-compatible shell interpreter.
    shell: Shell,
    /// The session transcript — every command and its output, in order.
    transcript: Transcript,
}

impl ClankShell {
    /// Create a new `ClankShell` with an empty transcript.
    pub async fn new() -> Self {
        let mut shell = Shell::builder()
            .default_builtins(BuiltinSet::BashMode)
            .shell_name("clank".to_string())
            .no_profile(true)
            .no_rc(true)
            .build()
            .await
            .expect("failed to create shell");
        clank_builtins::register(&mut shell);
        Self {
            shell,
            transcript: Transcript::new(),
        }
    }

    /// Run a single command line, capture its output, record both in the
    /// transcript, and return the outcome.
    ///
    /// Output is written to the process's real stdout after capture, so the
    /// operator sees it on the terminal.
    pub async fn run_command(&mut self, input: &str) -> CommandOutcome {
        // Record the command input into the transcript.
        self.transcript.push_command(input);

        // Set up a pipe to capture stdout and stderr.
        let (mut reader, writer) = io::pipe().expect("failed to create output pipe");
        let writer_clone = writer.try_clone().expect("failed to clone pipe writer");

        let mut params = self.shell.default_exec_params();
        params.set_fd(1, OpenFile::PipeWriter(writer));
        params.set_fd(2, OpenFile::PipeWriter(writer_clone));

        // Execute the command.
        let _ = self.shell.run_string(input, &params).await;

        // Close write ends by dropping params, then read captured output.
        drop(params);
        let mut output = String::new();
        reader
            .read_to_string(&mut output)
            .expect("failed to read command output");

        // Write captured output to the real terminal.
        print!("{output}");
        let _ = io::stdout().flush();

        // Record output in the transcript (empty output is ignored by push_output).
        self.transcript.push_output(&output);

        CommandOutcome {
            output,
            exit_code: self.shell.last_result(),
        }
    }

    /// Print the current transcript to stdout.
    ///
    /// Per the README: the output of `context show` is NOT recorded back into
    /// the transcript — it must not duplicate itself on inspection.
    pub fn context_show(&self) -> String {
        self.transcript.as_string()
    }

    /// Discard all transcript entries.
    pub fn context_clear(&mut self) {
        self.transcript.clear();
    }

    /// Drop the oldest `n` entries from the transcript.
    pub fn context_trim(&mut self, n: usize) {
        self.transcript.trim(n);
    }

    /// Return the full transcript as a string.
    ///
    /// This is the value passed to the AI model on each `ask` invocation.
    pub fn transcript_as_string(&self) -> String {
        self.transcript.as_string()
    }

    /// Returns the exit code of the last command run.
    pub fn last_result(&self) -> u8 {
        self.shell.last_result()
    }

    /// Execute a command string directly on the inner shell without capturing
    /// output or recording it in the transcript.
    ///
    /// Used by `clank-golden` for setup scripts that need to run in the same
    /// shell state as a test but whose output is not relevant to the test.
    pub async fn run_string_raw(
        &mut self,
        input: &str,
        params: &brush_core::ExecutionParameters,
    ) -> Result<brush_core::ExecutionResult, brush_core::Error> {
        self.shell.run_string(input, params).await
    }

    /// Returns the default execution parameters for the inner shell.
    ///
    /// Used by `clank-golden` to construct parameters for `run_string_raw`.
    pub fn default_exec_params(&self) -> brush_core::ExecutionParameters {
        self.shell.default_exec_params()
    }
}

// ── Public shell construction ─────────────────────────────────────────────────

/// Build a new clank shell instance.
///
/// Returns a `ClankShell` with an empty transcript and all clank builtins
/// registered. This is the primary entry point for shell construction.
pub async fn build_shell() -> ClankShell {
    ClankShell::new().await
}

// ── REPL ──────────────────────────────────────────────────────────────────────

/// Run an interactive read-eval-print loop over stdin until EOF or `exit`.
///
/// Handles `context` commands directly (shell-internal scope — not dispatched
/// through Brush). All other commands are dispatched through `ClankShell`.
///
/// The prompt is written to stderr so it does not pollute stdout.
pub async fn run_repl(mut shell: ClankShell) {
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

                match trimmed {
                    "exit" => break,

                    "context show" => {
                        // Output is written to stdout but NOT recorded in transcript.
                        let text = shell.context_show();
                        print!("{text}");
                        let _ = io::stdout().flush();
                    }

                    "context clear" => {
                        shell.context_clear();
                    }

                    s if s.starts_with("context trim ") => {
                        let rest = s.trim_start_matches("context trim ").trim();
                        match rest.parse::<usize>() {
                            Ok(n) => shell.context_trim(n),
                            Err(_) => eprintln!("clank: context trim: invalid argument: {rest}"),
                        }
                    }

                    "model list" => model_list(),

                    s if s.starts_with("model add ") => model_add(s),

                    s if s.starts_with("model default ") => model_set_default(s),

                    _ => {
                        shell.run_command(trimmed).await;
                    }
                }
            }
        }
    }
}

// ── model command handlers ────────────────────────────────────────────────────

/// `model list` — print configured providers and current default.
fn model_list() {
    match clank_config::load_config() {
        Err(e) => eprintln!("clank: model list: {e}"),
        Ok(config) => {
            if config.providers.is_empty() {
                println!("No providers configured.");
            } else {
                println!("Providers:");
                let mut names: Vec<&str> =
                    config.providers.keys().map(|s| s.as_str()).collect();
                names.sort();
                for name in names {
                    let key = &config.providers[name].api_key;
                    let redacted = redact_key(key);
                    println!("  {name}  (api_key: {redacted})");
                }
            }
            match &config.default_model {
                Some(m) => println!("\nDefault model: {m}"),
                None => println!("\nDefault model: (not set)"),
            }
        }
    }
}

/// `model add <provider> --key <key>` — register a provider and its API key.
fn model_add(input: &str) {
    // Parse: "model add <provider> --key <key>"
    let rest = input.trim_start_matches("model add ").trim();
    let parts: Vec<&str> = rest.splitn(3, ' ').collect();
    if parts.len() == 3 && parts[1] == "--key" {
        let provider = parts[0].to_string();
        let key = parts[2].to_string();
        match clank_config::load_config() {
            Err(e) => eprintln!("clank: model add: {e}"),
            Ok(mut config) => {
                config.add_provider(provider.clone(), key);
                match clank_config::save_config(&config) {
                    Ok(()) => println!("Provider '{provider}' added."),
                    Err(e) => eprintln!("clank: model add: {e}"),
                }
            }
        }
    } else {
        eprintln!("clank: usage: model add <provider> --key <key>");
    }
}

/// `model default <model>` — set the default model.
fn model_set_default(input: &str) {
    let model = input.trim_start_matches("model default ").trim().to_string();
    if model.is_empty() {
        eprintln!("clank: usage: model default <model>");
        return;
    }
    match clank_config::load_config() {
        Err(e) => eprintln!("clank: model default: {e}"),
        Ok(mut config) => {
            config.set_default_model(model.clone());
            match clank_config::save_config(&config) {
                Ok(()) => println!("Default model set to '{model}'."),
                Err(e) => eprintln!("clank: model default: {e}"),
            }
        }
    }
}

/// Partially redact an API key for display.
/// Shows only the last 5 characters; replaces the rest with `*`.
fn redact_key(key: &str) -> String {
    if key.len() <= 5 {
        return "*".repeat(key.len());
    }
    let visible = &key[key.len() - 5..];
    format!("{}{}", "*".repeat(key.len() - 5), visible)
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // -- ClankShell construction --

    #[tokio::test]
    async fn build_shell_succeeds() {
        let _shell = build_shell().await;
    }

    // -- run_command records transcript --

    #[tokio::test]
    async fn run_command_records_command_input() {
        let mut shell = build_shell().await;
        shell.run_command("true").await;
        let t = shell.transcript_as_string();
        assert!(t.contains("$ true"), "transcript should contain the command");
    }

    #[tokio::test]
    async fn run_command_records_output() {
        let mut shell = build_shell().await;
        shell.run_command("echo hello").await;
        let t = shell.transcript_as_string();
        assert!(t.contains("hello"), "transcript should contain command output");
    }

    #[tokio::test]
    async fn run_command_exit_code_zero_after_success() {
        let mut shell = build_shell().await;
        let outcome = shell.run_command("true").await;
        assert_eq!(outcome.exit_code, 0);
    }

    #[tokio::test]
    async fn run_command_exit_code_one_after_failure() {
        let mut shell = build_shell().await;
        let outcome = shell.run_command("false").await;
        assert_eq!(outcome.exit_code, 1);
    }

    // -- context_show --

    #[tokio::test]
    async fn context_show_returns_transcript() {
        let mut shell = build_shell().await;
        shell.run_command("echo hi").await;
        let shown = shell.context_show();
        assert!(shown.contains("$ echo hi"));
        assert!(shown.contains("hi"));
    }

    #[tokio::test]
    async fn context_show_does_not_grow_transcript() {
        let mut shell = build_shell().await;
        shell.run_command("echo hi").await;
        let len_before = shell.transcript.len();
        let _ = shell.context_show();
        assert_eq!(shell.transcript.len(), len_before, "context show must not record itself");
    }

    // -- context_clear --

    #[tokio::test]
    async fn context_clear_empties_transcript() {
        let mut shell = build_shell().await;
        shell.run_command("echo hi").await;
        shell.context_clear();
        assert!(shell.transcript.is_empty());
    }

    // -- context_trim --

    #[tokio::test]
    async fn context_trim_drops_oldest_entries() {
        let mut shell = build_shell().await;
        shell.run_command("echo first").await;
        shell.run_command("echo second").await;
        let len_before = shell.transcript.len();
        // Each run_command adds 2 entries (Command + Output).
        // Trim 2 to remove both entries for "first".
        shell.context_trim(2);
        assert!(shell.transcript.len() < len_before);
        assert!(shell.transcript_as_string().contains("second"));
    }
}
