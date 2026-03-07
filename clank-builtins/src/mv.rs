use std::io::Write;
use std::path::Path;

use brush_core::{ExecutionResult, commands::ExecutionContext};
use clap::Parser;

use crate::color;

/// clank's internal implementation of `mv`.
///
/// Moves or renames files and directories.
#[derive(Debug, Parser)]
pub struct MvCommand {
    /// Source and destination paths. Last argument is the destination.
    #[arg(required = true, num_args = 2..)]
    paths: Vec<String>,
}

impl brush_core::builtins::Command for MvCommand {
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
            writeln!(stderr, "{}mv:{} target '{dest}': Not a directory", color::CMD, color::RESET, dest = dest.display()).ok();
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

            if let Err(e) = std::fs::rename(src, &target) {
                // Cross-device rename: try copy + remove.
                if e.raw_os_error() == Some(18) || e.kind() == std::io::ErrorKind::Other {
                    if let Err(e2) = cross_device_move(src, &target) {
                        writeln!(stderr, "{}mv:{} {src_str}: {e2}", color::CMD, color::RESET).ok();
                        had_error = true;
                    }
                } else {
                    writeln!(stderr, "{}mv:{} {src_str}: {e}", color::CMD, color::RESET).ok();
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

fn cross_device_move(src: &Path, dest: &Path) -> std::io::Result<()> {
    if src.is_dir() {
        copy_dir_and_remove(src, dest)
    } else {
        std::fs::copy(src, dest)?;
        std::fs::remove_file(src)?;
        Ok(())
    }
}

fn copy_dir_and_remove(src: &Path, dest: &Path) -> std::io::Result<()> {
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
    std::fs::remove_dir_all(src)?;
    Ok(())
}
