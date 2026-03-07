use std::io::{BufRead, Write};

use brush_core::{ExecutionResult, commands::ExecutionContext};
use clap::Parser;

use crate::color;

/// clank's internal implementation of `head`.
///
/// Outputs the first N lines of each file (default 10). Reads stdin if no files given.
#[derive(Debug, Parser)]
pub struct HeadCommand {
    /// Number of lines to output.
    #[arg(short = 'n', default_value = "10")]
    lines: usize,

    /// Files to read. Defaults to stdin.
    #[arg()]
    files: Vec<String>,
}

impl brush_core::builtins::Command for HeadCommand {
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

        let show_header = files.len() > 1;

        for (i, file) in files.iter().enumerate() {
            if show_header {
                if i > 0 { writeln!(stdout).ok(); }
                writeln!(stdout, "==> {file} <==").ok();
            }

            if file == "-" {
                let stdin = context.stdin();
                let reader = std::io::BufReader::new(stdin);
                for line in reader.lines().take(self.lines) {
                    match line {
                        Ok(l) => { writeln!(stdout, "{l}").ok(); }
                        Err(e) => {
                            writeln!(stderr, "{}head:{} stdin: {e}", color::CMD, color::RESET).ok();
                            had_error = true;
                            break;
                        }
                    }
                }
            } else {
                match std::fs::File::open(file) {
                    Ok(f) => {
                        let reader = std::io::BufReader::new(f);
                        for line in reader.lines().take(self.lines) {
                            match line {
                                Ok(l) => { writeln!(stdout, "{l}").ok(); }
                                Err(e) => {
                                    writeln!(stderr, "{}head:{} {file}: {e}", color::CMD, color::RESET).ok();
                                    had_error = true;
                                    break;
                                }
                            }
                        }
                    }
                    Err(e) => {
                        writeln!(stderr, "{}head:{} {file}: {e}", color::CMD, color::RESET).ok();
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
