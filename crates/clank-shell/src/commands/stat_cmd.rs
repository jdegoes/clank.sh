use std::sync::Arc;

use async_trait::async_trait;
use clank_vfs::Vfs;

use crate::commands::resolve;
use crate::process::{Process, ProcessContext, ProcessResult};

pub struct StatProcess {
    vfs: Arc<dyn Vfs>,
}

impl StatProcess {
    pub fn new(vfs: Arc<dyn Vfs>) -> Self {
        Self { vfs }
    }
}

#[async_trait]
impl Process for StatProcess {
    async fn run(&self, mut ctx: ProcessContext) -> ProcessResult {
        let paths: Vec<String> = ctx.argv.iter().skip(1).cloned().collect();

        if paths.is_empty() {
            let _ = ctx.io.write_stderr(b"stat: missing operand\n");
            return ProcessResult::failure(1);
        }

        let mut exit_code = 0;
        for path in &paths {
            match self.vfs.stat(&resolve(&ctx.cwd, path)) {
                Ok(s) => {
                    let kind = if s.is_dir {
                        "directory"
                    } else if s.is_symlink {
                        "symbolic link"
                    } else {
                        "regular file"
                    };
                    let out = format!("  File: {path}\n  Size: {}\t\tType: {kind}\n", s.size);
                    let _ = ctx.io.write_stdout(out.as_bytes());
                }
                Err(e) => {
                    let _ = ctx
                        .io
                        .write_stderr(format!("stat: {path}: {e}\n").as_bytes());
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

    async fn run(argv: Vec<&str>, vfs: Arc<dyn Vfs>) -> (String, String, i32) {
        let out = tempfile::NamedTempFile::new().unwrap();
        let err = tempfile::NamedTempFile::new().unwrap();
        let ctx = ProcessContext {
            argv: argv.into_iter().map(str::to_string).collect(),
            env: HashMap::new(),
            io: ProcessIo {
                stdin: OpenFile::Stdin(std::io::stdin()),
                stdout: OpenFile::from(out.reopen().unwrap()),
                stderr: OpenFile::from(err.reopen().unwrap()),
            },
            pid: 0,
            cwd: std::path::PathBuf::from("/"),
        };
        let result = StatProcess::new(vfs).run(ctx).await;
        (
            std::fs::read_to_string(out.path()).unwrap(),
            std::fs::read_to_string(err.path()).unwrap(),
            result.exit_code,
        )
    }

    #[tokio::test]
    async fn test_stat_file_shows_size_and_type() {
        let vfs = Arc::new(MockVfs::new().with_file("/tmp/hello.txt", "hello world"));
        let (stdout, _, code) = run(vec!["stat", "/tmp/hello.txt"], vfs).await;
        assert_eq!(code, 0);
        assert!(stdout.contains("regular file"), "type missing: {stdout}");
        assert!(stdout.contains("11"), "size (11 bytes) missing: {stdout}");
    }

    #[tokio::test]
    async fn test_stat_missing_operand_exits_1() {
        let vfs = Arc::new(MockVfs::new());
        let (stdout, stderr, code) = run(vec!["stat"], vfs).await;
        assert_eq!(code, 1);
        assert!(stderr.contains("stat:"), "error prefix missing: {stderr}");
        assert!(stdout.is_empty());
    }

    #[tokio::test]
    async fn test_stat_missing_path_exits_1_with_error() {
        let vfs = Arc::new(MockVfs::new());
        let (stdout, stderr, code) = run(vec!["stat", "/no/such/file"], vfs).await;
        assert_eq!(code, 1);
        assert!(stderr.contains("stat:"), "error prefix missing: {stderr}");
        assert!(
            stderr.contains("/no/such/file"),
            "path missing from error: {stderr}"
        );
        assert!(stdout.is_empty());
    }
}
