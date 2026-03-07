use std::io::Write;

use brush_core::{ExecutionResult, commands::ExecutionContext};
use clap::Parser;

use crate::color;

/// clank's internal implementation of `rm`.
///
/// Removes files and directories. Use `-r` for recursive, `-f` to suppress errors.
#[derive(Debug, Parser)]
pub struct RmCommand {
    /// Remove directories and their contents recursively.
    #[arg(short = 'r', short_alias = 'R')]
    recursive: bool,

    /// Ignore nonexistent files and do not prompt.
    #[arg(short = 'f')]
    force: bool,

    /// Files or directories to remove.
    #[arg(required = true)]
    paths: Vec<String>,
}

impl brush_core::builtins::Command for RmCommand {
    type Error = brush_core::Error;

    async fn execute(
        &self,
        context: ExecutionContext<'_>,
    ) -> Result<ExecutionResult, Self::Error> {
        let mut stderr = context.stderr();
        let mut had_error = false;

        for path in &self.paths {
            let p = std::path::Path::new(path);
            let result = if p.is_dir() {
                if self.recursive {
                    std::fs::remove_dir_all(p)
                } else {
                    let e = std::io::Error::other("is a directory");
                    Err(e)
                }
            } else {
                std::fs::remove_file(p)
            };

            if let Err(e) = result {
                if !self.force {
                    writeln!(stderr, "{}rm:{} {path}: {e}", color::CMD, color::RESET).ok();
                    had_error = true;
                }
            }
        }

        if had_error {
            Ok(ExecutionResult::new(1))
        } else {
            Ok(ExecutionResult::success())
        }
    }
}
