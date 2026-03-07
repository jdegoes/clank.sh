use std::io::{BufRead, Write};

use brush_core::{ExecutionResult, commands::ExecutionContext};
use clap::Parser;

/// clank's internal implementation of `uniq`.
///
/// Filters adjacent duplicate lines. Use `-c` to prefix lines with occurrence count.
#[derive(Debug, Parser)]
pub struct UniqCommand {
    /// Prefix lines by the number of occurrences.
    #[arg(short = 'c')]
    count: bool,

    /// File to read. Defaults to stdin.
    #[arg()]
    file: Option<String>,
}

impl brush_core::builtins::Command for UniqCommand {
    type Error = brush_core::Error;

    async fn execute(
        &self,
        context: ExecutionContext<'_>,
    ) -> Result<ExecutionResult, Self::Error> {
        let mut stdout = context.stdout();
        let mut stderr = context.stderr();

        let lines: Vec<String> = match &self.file {
            Some(f) if f != "-" => {
                match std::fs::File::open(f) {
                    Ok(file) => {
                        std::io::BufReader::new(file)
                            .lines()
                            .map_while(Result::ok)
                            .collect()
                    }
                    Err(e) => {
                        writeln!(stderr, "uniq: {f}: {e}").ok();
                        return Ok(ExecutionResult::new(1));
                    }
                }
            }
            _ => {
                let stdin = context.stdin();
                let reader = std::io::BufReader::new(stdin);
                reader.lines().map_while(Result::ok).collect()
            }
        };

        if lines.is_empty() {
            return Ok(ExecutionResult::success());
        }

        let mut current = &lines[0];
        let mut run_count: usize = 1;

        for line in &lines[1..] {
            if line == current {
                run_count += 1;
            } else {
                output_line(current, run_count, self.count, &mut stdout);
                current = line;
                run_count = 1;
            }
        }
        output_line(current, run_count, self.count, &mut stdout);

        Ok(ExecutionResult::success())
    }
}

fn output_line(line: &str, count: usize, show_count: bool, stdout: &mut dyn Write) {
    if show_count {
        writeln!(stdout, "{count:>7} {line}").ok();
    } else {
        writeln!(stdout, "{line}").ok();
    }
}
