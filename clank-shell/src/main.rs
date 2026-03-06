//! clank — an AI-native Unix shell.
//!
//! This binary is the entry point for the clank shell. At this stage of the
//! bootstrap it simply boots a brush-core shell instance and executes a
//! command supplied on the command line (defaulting to `echo hello`).

use std::process::ExitCode;

use brush_core::{CreateOptions, Shell};

#[tokio::main]
async fn main() -> ExitCode {
    // Collect the command to run from argv, or fall back to "echo hello".
    let args: Vec<String> = std::env::args().skip(1).collect();
    let command = if args.is_empty() {
        "echo hello".to_owned()
    } else {
        args.join(" ")
    };

    match run(&command).await {
        Ok(exit_code) => ExitCode::from(exit_code),
        Err(err) => {
            eprintln!("clank: fatal error: {err}");
            ExitCode::FAILURE
        }
    }
}

/// Boot a shell and run a single command string, returning the numeric exit code.
async fn run(command: &str) -> Result<u8, brush_core::Error> {
    let options = CreateOptions {
        // Non-interactive: no profile/rc loading, no readline editing.
        interactive: false,
        no_profile: true,
        no_rc: true,
        no_editing: true,
        shell_name: Some("clank".to_owned()),
        ..CreateOptions::default()
    };

    let mut shell = Shell::new(options).await?;
    let params = shell.default_exec_params();
    let result = shell.run_string(command, &params).await?;
    Ok(result.exit_code.into())
}

#[cfg(test)]
mod tests {
    use super::*;

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
