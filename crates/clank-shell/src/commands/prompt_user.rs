use std::io::{BufRead, Read, Write};

use async_trait::async_trait;

use crate::process::{Process, ProcessContext, ProcessResult};
use crate::process_table::{self, ProcessStatus};

/// Implementation of the `prompt-user` shell-internal builtin.
///
/// Pauses the invoking process (enters `P` state), presents a Markdown-rendered
/// message and question to the human, and returns the response on stdout.
///
/// Flags:
///   --choices a,b,c   Constrain response to one of the listed options
///   --confirm         Shorthand for --choices yes,no
///   --secret          Suppress echo; response never enters transcript
pub struct PromptUserProcess {
    shell_id: u64,
}

impl PromptUserProcess {
    pub fn new(shell_id: u64) -> Self {
        Self { shell_id }
    }
}

#[async_trait]
impl Process for PromptUserProcess {
    async fn run(&self, mut ctx: ProcessContext) -> ProcessResult {
        // Parse flags.
        let confirm = ctx.argv.iter().any(|a| a == "--confirm");
        let secret = ctx.argv.iter().any(|a| a == "--secret");

        // Parse choices from argv before borrowing io.
        let choices: Option<Vec<String>> = if confirm {
            Some(vec!["yes".to_string(), "no".to_string()])
        } else {
            ctx.argv
                .iter()
                .find_map(|a| {
                    a.strip_prefix("--choices=")
                        .map(|v| v.split(',').map(str::to_string).collect())
                })
                .or_else(|| {
                    ctx.argv
                        .windows(2)
                        .find(|w| w[0] == "--choices")
                        .map(|w| w[1].split(',').map(str::to_string).collect())
                })
        };

        let question: String = ctx
            .argv
            .iter()
            .skip(1)
            .filter(|a| !a.starts_with('-') && !a.starts_with("--choices"))
            .cloned()
            .collect::<Vec<_>>()
            .join(" ");

        // Drain piped stdin as Markdown context.
        let mut markdown = String::new();
        let _ = ctx.io.stdin.read_to_string(&mut markdown);

        // Render Markdown context if present.
        if !markdown.trim().is_empty() {
            render_markdown(&markdown);
        }

        // Display the question.
        let stderr = std::io::stderr();
        let mut err = stderr.lock();

        if !question.is_empty() {
            let _ = writeln!(err, "\n{question}");
        }

        // Display choices.
        if let Some(ref opts) = choices {
            let _ = writeln!(err, "Options: {}", opts.join(", "));
        }

        // Enter P state in process table using the actual PID from the process table.
        process_table::set_status(self.shell_id, ctx.pid, ProcessStatus::Paused);

        // Read response from the real terminal stdin (not the piped ProcessIo stdin).
        let response = read_response(&choices, secret, &mut err);

        // Restore Running state (will be set to Zombie by dispatch_builtin after return).
        process_table::set_status(self.shell_id, ctx.pid, ProcessStatus::Running);

        match response {
            Ok(r) => {
                let _ = ctx.io.write_stdout(format!("{r}\n").as_bytes());
                // If secret, do NOT append to transcript — the caller must check the
                // `--secret` flag and suppress transcript recording.
                ProcessResult::success()
            }
            Err(130) => ProcessResult::failure(130), // Ctrl-C
            Err(code) => ProcessResult::failure(code),
        }
    }
}

/// Render Markdown to the terminal using termimad.
fn render_markdown(markdown: &str) {
    let skin = termimad::MadSkin::default();
    skin.print_text(markdown);
}

/// Read a validated response from terminal stdin.
fn read_response(
    choices: &Option<Vec<String>>,
    secret: bool,
    err: &mut impl Write,
) -> Result<String, i32> {
    let stdin = std::io::stdin();
    loop {
        let _ = write!(err, "> ");
        let _ = err.flush();

        let response = if secret {
            // Use rpassword to read without echoing (Dev 5).
            match rpassword::read_password() {
                Ok(pw) => pw,
                Err(_) => return Err(130),
            }
        } else {
            let mut line = String::new();
            match stdin.lock().read_line(&mut line) {
                Ok(0) | Err(_) => return Err(130), // EOF or error → treat as Ctrl-C
                Ok(_) => line.trim().to_string(),
            }
        };

        if response.is_empty() {
            continue;
        }
        // Validate against choices if specified.
        if let Some(opts) = choices {
            let valid = opts.iter().any(|o| o.eq_ignore_ascii_case(&response));
            if !valid {
                let _ = writeln!(err, "Please choose one of: {}", opts.join(", "));
                continue;
            }
        }
        return Ok(response);
    }
}
