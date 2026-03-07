use std::io::Read;
use std::sync::Arc;

use async_trait::async_trait;
use clank_vfs::Vfs;

use crate::commands::resolve;
use crate::process::{Process, ProcessContext, ProcessResult};

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

pub struct CatProcess {
    vfs: Arc<dyn Vfs>,
}

impl CatProcess {
    pub fn new(vfs: Arc<dyn Vfs>) -> Self {
        Self { vfs }
    }
}

#[async_trait]
impl Process for CatProcess {
    async fn run(&self, mut ctx: ProcessContext) -> ProcessResult {
        let paths: Vec<String> = ctx
            .argv
            .iter()
            .skip(1)
            .filter(|a| !a.starts_with('-'))
            .cloned()
            .collect();

        // If no paths given (or only flags), read from stdin.
        if paths.is_empty() {
            let mut buf = Vec::new();
            let _ = ctx.io.stdin.read_to_end(&mut buf);
            let _ = ctx.io.write_stdout(&buf);
            return ProcessResult::success();
        }

        let mut exit_code = 0;
        for path in &paths {
            match self.vfs.read_file(&resolve(&ctx.cwd, path)) {
                Ok(bytes) => {
                    let _ = ctx.io.write_stdout(&bytes);
                }
                Err(e) => {
                    let _ = ctx
                        .io
                        .write_stderr(format!("cat: {path}: {e}\n").as_bytes());
                    exit_code = 1;
                }
            }
        }

        if exit_code == 0 {
            ProcessResult::success()
        } else {
            ProcessResult::failure(exit_code)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::process::ProcessIo;
    use brush_core::openfiles::OpenFile;
    use clank_vfs::MockVfs;
    use std::collections::HashMap;
    use std::sync::Arc;

    fn make_ctx(argv: Vec<&str>, stdout: std::fs::File, stderr: std::fs::File) -> ProcessContext {
        ProcessContext {
            argv: argv.into_iter().map(str::to_string).collect(),
            env: HashMap::new(),
            io: ProcessIo {
                stdin: OpenFile::Stdin(std::io::stdin()),
                stdout: OpenFile::from(stdout),
                stderr: OpenFile::from(stderr),
            },
            pid: 0,
            cwd: std::path::PathBuf::from("/"),
        }
    }

    #[tokio::test]
    async fn test_cat_reads_file_via_vfs() {
        let vfs = Arc::new(MockVfs::new().with_file("/etc/hosts", "127.0.0.1 localhost\n"));
        let cat = CatProcess::new(vfs);

        let out = tempfile::NamedTempFile::new().unwrap();
        let err = tempfile::NamedTempFile::new().unwrap();
        let ctx = make_ctx(
            vec!["cat", "/etc/hosts"],
            out.reopen().unwrap(),
            err.reopen().unwrap(),
        );

        let result = cat.run(ctx).await;
        assert_eq!(result.exit_code, 0);
        let output = std::fs::read_to_string(out.path()).unwrap();
        assert_eq!(output, "127.0.0.1 localhost\n");
    }

    #[tokio::test]
    async fn test_cat_missing_file_exits_1_with_error_on_stderr() {
        let vfs = Arc::new(MockVfs::new());
        let cat = CatProcess::new(vfs);

        let out = tempfile::NamedTempFile::new().unwrap();
        let err = tempfile::NamedTempFile::new().unwrap();
        let ctx = make_ctx(
            vec!["cat", "/no/such/file"],
            out.reopen().unwrap(),
            err.reopen().unwrap(),
        );

        let result = cat.run(ctx).await;
        assert_eq!(result.exit_code, 1);
        let stderr = std::fs::read_to_string(err.path()).unwrap();
        assert!(stderr.contains("cat:"), "error prefix missing: {stderr}");
        assert!(
            stderr.contains("/no/such/file"),
            "path missing from error: {stderr}"
        );
        let stdout = std::fs::read_to_string(out.path()).unwrap();
        assert!(stdout.is_empty(), "stdout must be empty on error");
    }

    #[tokio::test]
    async fn test_cat_multiple_files_continues_after_error() {
        // cat /missing /present — should output present's content and exit 1.
        let vfs = Arc::new(MockVfs::new().with_file("/present", "content\n"));
        let cat = CatProcess::new(vfs);

        let out = tempfile::NamedTempFile::new().unwrap();
        let err = tempfile::NamedTempFile::new().unwrap();
        let ctx = make_ctx(
            vec!["cat", "/missing", "/present"],
            out.reopen().unwrap(),
            err.reopen().unwrap(),
        );

        let result = cat.run(ctx).await;
        assert_eq!(result.exit_code, 1, "must exit 1 when any file is missing");
        let stdout = std::fs::read_to_string(out.path()).unwrap();
        assert!(
            stdout.contains("content\n"),
            "present file content must still be emitted"
        );
    }
}
