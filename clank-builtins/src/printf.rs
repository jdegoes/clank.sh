use std::io::Write;

use brush_core::{ExecutionResult, commands::ExecutionContext};
use clap::Parser;

/// clank's internal implementation of `printf`.
///
/// Formats and prints arguments according to a format string.
/// Supports: `%s` (string), `%d` (integer), `%f` (float), `%%` (literal %),
/// `\n` (newline), `\t` (tab), `\\` (backslash).
#[derive(Debug, Parser)]
pub struct PrintfCommand {
    /// Format string followed by arguments.
    #[arg(required = true)]
    args: Vec<String>,
}

impl brush_core::builtins::Command for PrintfCommand {
    type Error = brush_core::Error;

    async fn execute(
        &self,
        context: ExecutionContext<'_>,
    ) -> Result<ExecutionResult, Self::Error> {
        let mut stdout = context.stdout();

        if self.args.is_empty() {
            return Ok(ExecutionResult::success());
        }

        let format = &self.args[0];
        let args = &self.args[1..];
        let output = format_string(format, args);
        write!(stdout, "{output}").ok();

        Ok(ExecutionResult::success())
    }
}

fn format_string(format: &str, args: &[String]) -> String {
    let mut result = String::new();
    let mut chars = format.chars().peekable();
    let mut arg_index = 0;

    while let Some(ch) = chars.next() {
        match ch {
            '\\' => {
                match chars.next() {
                    Some('n') => result.push('\n'),
                    Some('t') => result.push('\t'),
                    Some('\\') => result.push('\\'),
                    Some('0') => result.push('\0'),
                    Some(other) => {
                        result.push('\\');
                        result.push(other);
                    }
                    None => result.push('\\'),
                }
            }
            '%' => {
                match chars.peek() {
                    Some('%') => {
                        chars.next();
                        result.push('%');
                    }
                    Some('s') => {
                        chars.next();
                        let val = args.get(arg_index).map(|s| s.as_str()).unwrap_or("");
                        result.push_str(val);
                        arg_index += 1;
                    }
                    Some('d') => {
                        chars.next();
                        let val = args
                            .get(arg_index)
                            .and_then(|s| s.parse::<i64>().ok())
                            .unwrap_or(0);
                        result.push_str(&val.to_string());
                        arg_index += 1;
                    }
                    Some('f') => {
                        chars.next();
                        let val = args
                            .get(arg_index)
                            .and_then(|s| s.parse::<f64>().ok())
                            .unwrap_or(0.0);
                        result.push_str(&format!("{val:.6}"));
                        arg_index += 1;
                    }
                    _ => result.push('%'),
                }
            }
            other => result.push(other),
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_string_simple() {
        let result = format_string("hello %s\\n", &["world".to_string()]);
        assert_eq!(result, "hello world\n");
    }

    #[test]
    fn format_integer() {
        let result = format_string("%d items", &["42".to_string()]);
        assert_eq!(result, "42 items");
    }

    #[test]
    fn format_float() {
        let result = format_string("%f", &["3.14".to_string()]);
        assert_eq!(result, "3.140000");
    }

    #[test]
    fn format_percent_literal() {
        let result = format_string("100%%", &[]);
        assert_eq!(result, "100%");
    }

    #[test]
    fn format_missing_args() {
        let result = format_string("%s and %s", &["hello".to_string()]);
        assert_eq!(result, "hello and ");
    }

    #[test]
    fn format_tab() {
        let result = format_string("a\\tb", &[]);
        assert_eq!(result, "a\tb");
    }
}
