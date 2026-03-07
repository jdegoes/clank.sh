use std::io::Write;

use brush_core::{ExecutionResult, commands::ExecutionContext};
use clap::Parser;

use crate::color;

/// clank's internal implementation of `sleep`.
///
/// Pauses for the specified number of seconds. Supports decimal values.
#[derive(Debug, Parser)]
pub struct SleepCommand {
    /// Number of seconds to sleep (supports decimals, e.g. 0.5).
    #[arg(required = true)]
    seconds: f64,
}

impl brush_core::builtins::Command for SleepCommand {
    type Error = brush_core::Error;

    async fn execute(
        &self,
        context: ExecutionContext<'_>,
    ) -> Result<ExecutionResult, Self::Error> {
        if self.seconds < 0.0 {
            let mut stderr = context.stderr();
            writeln!(stderr, "{}sleep:{} invalid time interval: {}", color::CMD, color::RESET, self.seconds).ok();
            return Ok(ExecutionResult::new(1));
        }

        let duration = std::time::Duration::from_secs_f64(self.seconds);
        tokio::time::sleep(duration).await;

        Ok(ExecutionResult::success())
    }
}
