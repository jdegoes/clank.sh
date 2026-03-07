use std::io::Write;
use std::path::Path;

use brush_core::{ExecutionResult, commands::ExecutionContext};
use clap::Parser;

/// clank's internal implementation of `cp`.
///
/// Copies files and directories. Use `-r` for recursive directory copy.
#[derive(Debug, Parser)]
pub struct CpCommand {
    /// Copy directories recursively.
    #[arg(short = 'r', short_alias = 'R')]
    recursive: bool,

    /// Source and destination paths. Last argument is the destination.
    #[arg(required = true, num_args = 2..)]
    paths: Vec<String>,
}

impl brush_core::builtins::Command for CpCommand {
    type Error = brush_core::Error;

    async fn execute(
        &self,
        context: ExecutionContext<'_>,
    ) -> Result<ExecutionResult, Self::Error> {
        let mut stderr = context.stderr();

        let (sources, dest) = self.paths.split_at(self.paths.len() - 1);
        let dest = Path::new(&dest[0]);

        let multiple_sources = sources.len() > 1;
        if multiple_sources && !dest.is_dir() {
            writeln!(stderr, "cp: target '{dest}': Not a directory", dest = dest.display()).ok();
            return Ok(ExecutionResult::new(1));
        }

        let mut had_error = false;

        for src_str in sources {
            let src = Path::new(src_str);
            let target = if dest.is_dir() {
                dest.join(src.file_name().unwrap_or(src.as_ref()))
            } else {
                dest.to_path_buf()
            };

            if src.is_dir() {
                if !self.recursive {
                    writeln!(stderr, "cp: -r not specified; omitting directory '{src_str}'").ok();
                    had_error = true;
                    continue;
                }
                if let Err(e) = copy_dir_recursive(src, &target) {
                    writeln!(stderr, "cp: {src_str}: {e}").ok();
                    had_error = true;
                }
            } else if let Err(e) = std::fs::copy(src, &target) {
                writeln!(stderr, "cp: {src_str}: {e}").ok();
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

fn copy_dir_recursive(src: &Path, dest: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dest)?;
    for entry in walkdir::WalkDir::new(src).min_depth(1) {
        let entry = entry.map_err(std::io::Error::other)?;
        let relative = entry.path().strip_prefix(src).unwrap();
        let target = dest.join(relative);
        if entry.file_type().is_dir() {
            std::fs::create_dir_all(&target)?;
        } else {
            std::fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}
