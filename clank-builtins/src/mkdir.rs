use std::io::Write;

use brush_core::{ExecutionResult, commands::ExecutionContext};
use clap::Parser;

/// clank's internal implementation of `mkdir`.
///
/// Creates directories. Use `-p` to create parent directories as needed.
#[derive(Debug, Parser)]
pub struct MkdirCommand {
    /// Create parent directories as needed.
    #[arg(short = 'p')]
    parents: bool,

    /// Directories to create.
    #[arg(required = true)]
    dirs: Vec<String>,
}

impl brush_core::builtins::Command for MkdirCommand {
    type Error = brush_core::Error;

    async fn execute(
        &self,
        context: ExecutionContext<'_>,
    ) -> Result<ExecutionResult, Self::Error> {
        let mut stderr = context.stderr();
        let mut had_error = false;

        for dir in &self.dirs {
            let result = if self.parents {
                std::fs::create_dir_all(dir)
            } else {
                std::fs::create_dir(dir)
            };
            if let Err(e) = result {
                writeln!(stderr, "mkdir: {dir}: {e}").ok();
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
