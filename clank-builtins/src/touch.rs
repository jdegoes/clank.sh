use std::fs::OpenOptions;
use std::io::Write;

use brush_core::{ExecutionResult, commands::ExecutionContext};
use clap::Parser;

use crate::color;

/// clank's internal implementation of `touch`.
///
/// Creates empty files or updates modification time of existing files.
#[derive(Debug, Parser)]
pub struct TouchCommand {
    /// Files to create or update.
    #[arg(required = true)]
    files: Vec<String>,
}

impl brush_core::builtins::Command for TouchCommand {
    type Error = brush_core::Error;

    async fn execute(
        &self,
        context: ExecutionContext<'_>,
    ) -> Result<ExecutionResult, Self::Error> {
        let mut stderr = context.stderr();
        let mut had_error = false;

        for file in &self.files {
            let result = OpenOptions::new()
                .create(true)
                .append(true)
                .open(file);

            if let Err(e) = result {
                writeln!(stderr, "{}touch:{} {file}: {e}", color::CMD, color::RESET).ok();
                had_error = true;
            }
        }

        if had_error {
            Ok(ExecutionResult::new(1))
        } else {
            Ok(ExecutionResult::success())
        }
    }
}
