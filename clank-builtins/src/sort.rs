use std::io::{BufRead, Write};

use brush_core::{ExecutionResult, commands::ExecutionContext};
use clap::Parser;

/// clank's internal implementation of `sort`.
///
/// Sorts lines from files or stdin.
#[derive(Debug, Parser)]
pub struct SortCommand {
    /// Reverse the sort order.
    #[arg(short = 'r')]
    reverse: bool,

    /// Sort numerically instead of lexicographically.
    #[arg(short = 'n')]
    numeric: bool,

    /// Files to sort. Defaults to stdin.
    #[arg()]
    files: Vec<String>,
}

impl brush_core::builtins::Command for SortCommand {
    type Error = brush_core::Error;

    async fn execute(
        &self,
        context: ExecutionContext<'_>,
    ) -> Result<ExecutionResult, Self::Error> {
        let mut stdout = context.stdout();
        let mut stderr = context.stderr();
        let mut had_error = false;
        let mut all_lines: Vec<String> = Vec::new();

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
                        Ok(l) => all_lines.push(l),
                        Err(_) => break,
                    }
                }
            } else {
                match std::fs::File::open(file) {
                    Ok(f) => {
                        let reader = std::io::BufReader::new(f);
                        for line in reader.lines() {
                            match line {
                                Ok(l) => all_lines.push(l),
                                Err(e) => {
                                    writeln!(stderr, "sort: {file}: {e}").ok();
                                    had_error = true;
                                    break;
                                }
                            }
                        }
                    }
                    Err(e) => {
                        writeln!(stderr, "sort: {file}: {e}").ok();
                        had_error = true;
                    }
                }
            }
        }

        if self.numeric {
            all_lines.sort_by(|a, b| {
                let na = parse_leading_number(a);
                let nb = parse_leading_number(b);
                na.partial_cmp(&nb).unwrap_or(std::cmp::Ordering::Equal)
            });
        } else {
            all_lines.sort();
        }

        if self.reverse {
            all_lines.reverse();
        }

        for line in &all_lines {
            writeln!(stdout, "{line}").ok();
        }

        if had_error {
            Ok(ExecutionResult::new(1))
        } else {
            Ok(ExecutionResult::success())
        }
    }
}

/// Parse the leading numeric value from a string for numeric sort.
/// Non-numeric strings sort as 0 (matching GNU sort behaviour).
fn parse_leading_number(s: &str) -> f64 {
    let trimmed = s.trim_start();
    let numeric_part: String = trimmed
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.' || *c == '-' || *c == '+')
        .collect();
    numeric_part.parse::<f64>().unwrap_or(0.0)
}
