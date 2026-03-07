use brush_core::{ExecutionResult, commands::ExecutionContext};
use clap::Parser;

/// clank's internal implementation of `false`.
///
/// Always exits with code 1. No OS process is spawned.
#[derive(Debug, Parser)]
pub struct FalseCommand;

impl brush_core::builtins::Command for FalseCommand {
    type Error = brush_core::Error;

    async fn execute(
        &self,
        _context: ExecutionContext<'_>,
    ) -> Result<ExecutionResult, Self::Error> {
        Ok(ExecutionResult::new(1))
    }
}
