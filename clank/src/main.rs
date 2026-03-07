use std::io::{self, BufRead, Write};

use brush_builtins::{BuiltinSet, ShellBuilderExt};
use brush_core::Shell;

#[tokio::main]
async fn main() {
    let shell = Shell::builder()
        .default_builtins(BuiltinSet::BashMode)
        .shell_name("clank".to_string())
        .no_profile(true)
        .no_rc(true)
        .build()
        .await
        .expect("failed to create shell");

    run_repl(shell).await;
}

async fn run_repl(mut shell: Shell) {
    let stdin = io::stdin();
    let mut lines = stdin.lock().lines();

    loop {
        // Print prompt to stderr so it doesn't interfere with stdout in tests.
        eprint!("$ ");
        let _ = io::stderr().flush();

        match lines.next() {
            None => {
                // EOF (Ctrl-D)
                break;
            }
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
                match shell.run_string(trimmed, &params).await {
                    Ok(_) => {}
                    Err(e) => {
                        eprintln!("clank: {e}");
                    }
                }
            }
        }
    }
}
