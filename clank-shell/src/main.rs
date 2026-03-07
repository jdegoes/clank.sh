//! clank — entry point for the clank shell binary.
//!
//! This file is intentionally thin. All shell logic lives in `clank-core`.
//!
//! ## Execution modes
//!
//! - **Argv mode** — one or more arguments are provided; they are joined with
//!   spaces and executed as a command string.
//! - **Stdin mode** — no arguments provided; the full contents of stdin are
//!   read and executed as a command string. This is the mode used by the
//!   acceptance test harness, which pipes script bodies via stdin.

use std::io::Read as _;
use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();

    let command = if args.is_empty() {
        // Stdin mode: read the entire stdin and execute it.
        let mut buf = String::new();
        if let Err(err) = std::io::stdin().read_to_string(&mut buf) {
            eprintln!("clank: failed to read stdin: {err}");
            return ExitCode::FAILURE;
        }
        buf
    } else {
        // Argv mode: join all arguments as a single command string.
        args.join(" ")
    };

    match clank_core::run(&command).await {
        Ok(exit_code) => ExitCode::from(exit_code),
        Err(err) => {
            eprintln!("clank: fatal error: {err}");
            ExitCode::FAILURE
        }
    }
}
