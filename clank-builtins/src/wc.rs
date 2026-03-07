use std::io::{BufRead, Write};

use brush_core::{ExecutionResult, commands::ExecutionContext};
use clap::Parser;

/// Counts for a single input source.
struct Counts {
    lines: usize,
    words: usize,
    bytes: usize,
}

/// clank's internal implementation of `wc`.
///
/// Counts lines, words, and bytes. Reads stdin if no files given.
#[derive(Debug, Parser)]
pub struct WcCommand {
    /// Print the line count.
    #[arg(short = 'l')]
    count_lines: bool,

    /// Print the word count.
    #[arg(short = 'w')]
    count_words: bool,

    /// Print the byte count.
    #[arg(short = 'c')]
    count_bytes: bool,

    /// Files to count. Defaults to stdin.
    #[arg()]
    files: Vec<String>,
}

impl brush_core::builtins::Command for WcCommand {
    type Error = brush_core::Error;

    async fn execute(
        &self,
        context: ExecutionContext<'_>,
    ) -> Result<ExecutionResult, Self::Error> {
        let mut stdout = context.stdout();
        let mut stderr = context.stderr();
        let mut had_error = false;

        // If no specific flag is set, show all three.
        let show_all = !self.count_lines && !self.count_words && !self.count_bytes;
        let show_lines = self.count_lines || show_all;
        let show_words = self.count_words || show_all;
        let show_bytes = self.count_bytes || show_all;

        let files = if self.files.is_empty() {
            vec!["-".to_string()]
        } else {
            self.files.clone()
        };

        let mut total = Counts { lines: 0, words: 0, bytes: 0 };
        let show_total = files.len() > 1;

        for file in &files {
            if file == "-" {
                let stdin = context.stdin();
                let reader = std::io::BufReader::new(stdin);
                let mut content = String::new();
                for line in reader.lines() {
                    match line {
                        Ok(l) => {
                            content.push_str(&l);
                            content.push('\n');
                        }
                        Err(_) => break,
                    }
                }
                let counts = count_content(&content);
                format_counts(&counts, show_lines, show_words, show_bytes, None, &mut stdout);
                total.lines += counts.lines;
                total.words += counts.words;
                total.bytes += counts.bytes;
            } else {
                match std::fs::read(file) {
                    Ok(bytes) => {
                        let content = String::from_utf8_lossy(&bytes);
                        let counts = count_content(&content);
                        format_counts(&counts, show_lines, show_words, show_bytes, Some(file), &mut stdout);
                        total.lines += counts.lines;
                        total.words += counts.words;
                        total.bytes += counts.bytes;
                    }
                    Err(e) => {
                        writeln!(stderr, "wc: {file}: {e}").ok();
                        had_error = true;
                    }
                }
            }
        }

        if show_total {
            format_counts(&total, show_lines, show_words, show_bytes, Some("total"), &mut stdout);
        }

        if had_error {
            Ok(ExecutionResult::new(1))
        } else {
            Ok(ExecutionResult::success())
        }
    }
}

fn count_content(content: &str) -> Counts {
    let lines = content.lines().count();
    let words = content.split_whitespace().count();
    let bytes = content.len();
    Counts { lines, words, bytes }
}

fn format_counts(
    counts: &Counts,
    show_lines: bool,
    show_words: bool,
    show_bytes: bool,
    label: Option<&str>,
    stdout: &mut dyn Write,
) {
    let mut parts = Vec::new();
    if show_lines { parts.push(format!("{:>8}", counts.lines)); }
    if show_words { parts.push(format!("{:>8}", counts.words)); }
    if show_bytes { parts.push(format!("{:>8}", counts.bytes)); }

    let line = parts.join("");
    match label {
        Some(name) => writeln!(stdout, "{line} {name}").ok(),
        None => writeln!(stdout, "{line}").ok(),
    };
}
