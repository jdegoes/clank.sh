use std::io::Write;

use brush_core::{ExecutionResult, commands::ExecutionContext};
use clap::Parser;

/// clank's internal implementation of `env`.
///
/// Prints all environment variables as KEY=VALUE lines.
#[derive(Debug, Parser)]
pub struct EnvCommand;

impl brush_core::builtins::Command for EnvCommand {
    type Error = brush_core::Error;

    async fn execute(
        &self,
        context: ExecutionContext<'_>,
    ) -> Result<ExecutionResult, Self::Error> {
        let mut stdout = context.stdout();

        for (key, value) in std::env::vars() {
            writeln!(stdout, "{key}={value}").ok();
        }

        Ok(ExecutionResult::success())
    }
}
