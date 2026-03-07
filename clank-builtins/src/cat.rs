use std::io::{BufRead, Write};

use brush_core::{ExecutionResult, commands::ExecutionContext};
use clap::Parser;

use crate::color;

/// clank's internal implementation of `cat`.
///
/// Concatenates files and writes to stdout. Use `-` to read from stdin.
/// This implementation runs as an internal Rust function — no OS process is spawned.
#[derive(Debug, Parser)]
pub struct CatCommand {
    /// Files to concatenate. Use `-` for stdin. Defaults to stdin if no files given.
    #[arg()]
    files: Vec<String>,
}

impl brush_core::builtins::Command for CatCommand {
    type Error = brush_core::Error;

    async fn execute(
        &self,
        context: ExecutionContext<'_>,
    ) -> Result<ExecutionResult, Self::Error> {
        let mut stdout = context.stdout();
        let mut stderr = context.stderr();
        let mut had_error = false;

        let files = if self.files.is_empty() {
            vec!["-".to_string()]
        } else {
            self.files.clone()
        };

        for file in &files {
            if file == "-" {
                let stdin = context.stdin();
                let reader = std::io::BufReader::new(stdin);
                for line in reader.lines() {
                    match line {
                        Ok(l) => { writeln!(stdout, "{l}").ok(); }
                        Err(e) => {
                            writeln!(stderr, "{}cat:{} stdin: {e}", color::CMD, color::RESET).ok();
                            had_error = true;
                            break;
                        }
                    }
                }
            } else {
                match std::fs::read(file) {
                    Ok(contents) => { stdout.write_all(&contents).ok(); }
                    Err(e) => {
                        writeln!(stderr, "{}cat:{} {file}: {e}", color::CMD, color::RESET).ok();
                        had_error = true;
                    }
                }
            }
        }

        if had_error {
            Ok(ExecutionResult::new(1))
        } else {
            Ok(ExecutionResult::success())
        }
    }
}
