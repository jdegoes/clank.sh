use std::io::Write;

use brush_core::{ExecutionResult, commands::ExecutionContext};
use clap::Parser;

/// clank's internal implementation of `echo`.
///
/// Writes its arguments joined by spaces to stdout, followed by a newline.
/// This implementation runs as an internal Rust function — no OS process is spawned.
#[derive(Debug, Parser)]
pub struct EchoCommand {
    /// Arguments to print
    args: Vec<String>,
}

impl brush_core::builtins::Command for EchoCommand {
    type Error = brush_core::Error;

    async fn execute(
        &self,
        context: ExecutionContext<'_>,
    ) -> Result<ExecutionResult, Self::Error> {
        let mut stdout = context.stdout();
        let output = self.args.join(" ");
        writeln!(stdout, "{output}").ok();
        Ok(ExecutionResult::success())
    }
}
