use brush_core::{ExecutionResult, commands::ExecutionContext};
use clap::Parser;

/// clank's internal implementation of `true`.
///
/// Always exits with code 0. No OS process is spawned.
#[derive(Debug, Parser)]
pub struct TrueCommand;

impl brush_core::builtins::Command for TrueCommand {
    type Error = brush_core::Error;

    async fn execute(
        &self,
        _context: ExecutionContext<'_>,
    ) -> Result<ExecutionResult, Self::Error> {
        Ok(ExecutionResult::success())
    }
}
