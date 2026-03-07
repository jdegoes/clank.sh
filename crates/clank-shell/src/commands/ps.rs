use async_trait::async_trait;

use crate::process::{Process, ProcessContext, ProcessResult};
use crate::process_table;

pub struct PsProcess {
    shell_id: u64,
}

impl PsProcess {
    pub fn new(shell_id: u64) -> Self {
        Self { shell_id }
    }
}

#[async_trait]
impl Process for PsProcess {
    async fn run(&self, mut ctx: ProcessContext) -> ProcessResult {
        let entries = process_table::snapshot(self.shell_id);

        // Determine output format: `ps aux`/`ps ax` vs `ps -ef` vs simple `ps`
        let is_aux = ctx.argv.iter().any(|a| a == "aux" || a == "ax");
        let is_ef = ctx.argv.iter().any(|a| a == "-ef");

        // Header
        let header = if is_aux {
            "USER       PID  %CPU %MEM   VSZ   RSS TTY  STAT START TIME COMMAND\n"
        } else if is_ef {
            "UID        PID  PPID C STIME TTY  TIME CMD\n"
        } else {
            "  PID STAT COMMAND\n"
        };
        let _ = ctx.io.write_stdout(header.as_bytes());

        let user = std::env::var("USER").unwrap_or_else(|_| "user".to_string());

        let mut sorted = entries;
        sorted.sort_by_key(|e| e.pid);

        for entry in &sorted {
            let state = entry.status.state_char();
            let cmd = entry.argv.join(" ");
            let line = if is_aux {
                // USER PID %CPU %MEM VSZ RSS TTY STAT START TIME COMMAND
                // Non-meaningful columns show `-`.
                format!(
                    "{:<10} {:<5} {:<4} {:<4} {:<5} {:<5} {:<4} {:<4} {:<5} {:<4} {}\n",
                    user, entry.pid, "-", "-", "-", "-", "-", state, "-", "-", cmd
                )
            } else if is_ef {
                // UID PID PPID C STIME TTY TIME CMD
                format!(
                    "{:<10} {:<5} {:<4} {:<1} {:<5} {:<4} {:<8} {}\n",
                    user, entry.pid, entry.ppid, "-", "-", "-", "-", cmd
                )
            } else {
                format!("{:>5} {}    {}\n", entry.pid, state, cmd)
            };
            let _ = ctx.io.write_stdout(line.as_bytes());
        }

        ProcessResult::success()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::process::ProcessIo;
    use brush_core::openfiles::OpenFile;
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn unique_shell_id() -> u64 {
        static COUNTER: AtomicU64 = AtomicU64::new(50_000);
        COUNTER.fetch_add(1, Ordering::Relaxed)
    }

    /// Build a ProcessContext with a pipe-backed stdout for output capture.
    fn make_ctx(argv: Vec<&str>, stdout_writer: std::fs::File) -> ProcessContext {
        ProcessContext {
            argv: argv.into_iter().map(str::to_string).collect(),
            env: HashMap::new(),
            io: ProcessIo {
                stdin: OpenFile::Stdin(std::io::stdin()),
                stdout: OpenFile::from(stdout_writer),
                stderr: OpenFile::Stderr(std::io::stderr()),
            },
            pid: 0,
            cwd: std::path::PathBuf::from("/"),
        }
    }

    /// D6: `ps aux` header contains `%CPU` and `%MEM`.
    #[tokio::test]
    async fn test_ps_aux_header_has_cpu_mem_columns() {
        let sid = unique_shell_id();
        let ps = PsProcess::new(sid);

        let tmp = tempfile::NamedTempFile::new().expect("tmp");
        let writer = tmp.reopen().expect("reopen");
        let ctx = make_ctx(vec!["ps", "aux"], writer);

        ps.run(ctx).await;

        let output = std::fs::read_to_string(tmp.path()).unwrap_or_default();
        assert!(
            output.contains("%CPU"),
            "ps aux header missing %CPU; got: {output:?}"
        );
        assert!(
            output.contains("%MEM"),
            "ps aux header missing %MEM; got: {output:?}"
        );
    }

    /// D6: `ps -ef` header contains `UID`, `PPID`, `STIME`.
    #[tokio::test]
    async fn test_ps_ef_header_has_standard_columns() {
        let sid = unique_shell_id();
        let ps = PsProcess::new(sid);

        let tmp = tempfile::NamedTempFile::new().expect("tmp");
        let writer = tmp.reopen().expect("reopen");
        let ctx = make_ctx(vec!["ps", "-ef"], writer);

        ps.run(ctx).await;

        let output = std::fs::read_to_string(tmp.path()).unwrap_or_default();
        assert!(
            output.contains("UID"),
            "ps -ef header missing UID; got: {output:?}"
        );
        assert!(
            output.contains("PPID"),
            "ps -ef header missing PPID; got: {output:?}"
        );
    }
}
