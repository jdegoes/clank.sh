//! clank — entry point for the clank shell binary.
//!
//! This file is intentionally thin. All shell logic lives in `clank-core`.
//!
//! ## Execution modes
//!
//! - **Argv mode** — one or more arguments are provided; they are joined with
//!   spaces and executed as a single command string.
//! - **Script mode** — no arguments, stdin is not a TTY; the full contents of
//!   stdin are read and executed as a command string. This is the mode used by
//!   the acceptance test harness, which pipes script bodies via stdin.
//! - **Interactive mode** — no arguments, stdin is a TTY; a REPL loop reads
//!   one line at a time, prints a prompt, and executes each command until EOF
//!   or an explicit `exit`.

use std::io::{BufReader, IsTerminal as _, Read as _};
use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();

    if !args.is_empty() {
        // Argv mode: join all arguments as a single command string.
        let command = args.join(" ");
        return match clank_core::run(&command).await {
            Ok(exit_code) => ExitCode::from(exit_code),
            Err(err) => {
                eprintln!("clank: fatal error: {err}");
                ExitCode::FAILURE
            }
        };
    }

    let stdin = std::io::stdin();

    if !stdin.is_terminal() {
        // Script mode: read entire stdin and execute as a script.
        let mut buf = String::new();
        if let Err(err) = stdin.lock().read_to_string(&mut buf) {
            eprintln!("clank: failed to read stdin: {err}");
            return ExitCode::FAILURE;
        }
        return match clank_core::run(&buf).await {
            Ok(exit_code) => ExitCode::from(exit_code),
            Err(err) => {
                eprintln!("clank: fatal error: {err}");
                ExitCode::FAILURE
            }
        };
    }

    // Interactive mode: stdin is a TTY — run the REPL loop.
    let mut shell = match clank_core::Shell::new(clank_core::interactive_options()).await {
        Ok(s) => s,
        Err(err) => {
            eprintln!("clank: failed to start shell: {err}");
            return ExitCode::FAILURE;
        }
    };

    let reader = BufReader::new(stdin.lock());
    match clank_core::run_interactive(&mut shell, reader, std::io::stdout()).await {
        Ok(exit_code) => ExitCode::from(exit_code),
        Err(err) => {
            eprintln!("clank: fatal error: {err}");
            ExitCode::FAILURE
        }
    }
}
