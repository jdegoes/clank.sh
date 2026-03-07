use std::io::{BufRead, Write};

use brush_core::{ExecutionResult, commands::ExecutionContext};
use clap::Parser;

use crate::color;

/// clank's internal implementation of `tail`.
///
/// Outputs the last N lines of each file (default 10). Reads stdin if no files given.
#[derive(Debug, Parser)]
pub struct TailCommand {
    /// Number of lines to output.
    #[arg(short = 'n', default_value = "10")]
    lines: usize,

    /// Files to read. Defaults to stdin.
    #[arg()]
    files: Vec<String>,
}

impl brush_core::builtins::Command for TailCommand {
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
                let all_lines: Vec<String> = reader
                    .lines()
                    .map_while(Result::ok)
                    .collect();
                output_tail_lines(&all_lines, self.lines, &mut stdout);
            } else {
                match std::fs::File::open(file) {
                    Ok(f) => {
                        let reader = std::io::BufReader::new(f);
                        let all_lines: Vec<String> = reader
                            .lines()
                            .map_while(Result::ok)
                            .collect();
                        output_tail_lines(&all_lines, self.lines, &mut stdout);
                    }
                    Err(e) => {
                        writeln!(stderr, "{}tail:{} {file}: {e}", color::CMD, color::RESET).ok();
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

fn output_tail_lines(lines: &[String], n: usize, stdout: &mut dyn Write) {
    let start = lines.len().saturating_sub(n);
    for line in &lines[start..] {
        writeln!(stdout, "{line}").ok();
    }
}
