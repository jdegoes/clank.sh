use std::sync::Arc;

use async_trait::async_trait;
use clank_vfs::Vfs;

use crate::commands::resolve;
use crate::process::{Process, ProcessContext, ProcessResult};

pub struct MkdirProcess {
    vfs: Arc<dyn Vfs>,
}

impl MkdirProcess {
    pub fn new(vfs: Arc<dyn Vfs>) -> Self {
        Self { vfs }
    }
}

#[async_trait]
impl Process for MkdirProcess {
    async fn run(&self, mut ctx: ProcessContext) -> ProcessResult {
        let parents = has_flag(&ctx.argv, 'p', Some("--parents"));
        let paths: Vec<String> = ctx
            .argv
            .iter()
            .skip(1)
            .filter(|a| !a.starts_with('-'))
            .cloned()
            .collect();

        if paths.is_empty() {
            let _ = ctx.io.write_stderr(b"mkdir: missing operand\n");
            return ProcessResult::failure(1);
        }

        let mut exit_code = 0;
        for path in &paths {
            let resolved = resolve(&ctx.cwd, path);
            let result = if parents {
                self.vfs.create_dir_all(&resolved)
            } else {
                self.vfs.create_dir(&resolved)
            };
            if let Err(e) = result {
                let _ = ctx
                    .io
                    .write_stderr(format!("mkdir: {path}: {e}\n").as_bytes());
                exit_code = 1;
            }
        }

        if exit_code == 0 {
            ProcessResult::success()
        } else {
            ProcessResult::failure(exit_code)
        }
    }
}

/// Check whether a flag is present in argv, supporting combined short flags
/// (e.g. `-rp`) as well as standalone (`-p`) and long (`--parents`).
pub(crate) fn has_flag(argv: &[String], short: char, long: Option<&str>) -> bool {
    argv.iter().any(|a| {
        if let Some(l) = long {
            if a == l {
                return true;
            }
        }
        if a.starts_with('-') && !a.starts_with("--") {
            return a[1..].contains(short);
        }
        false
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::process::ProcessIo;
    use brush_core::openfiles::OpenFile;
    use clank_vfs::MockVfs;
    use std::collections::HashMap;
    use std::sync::Arc;

    async fn run_with_vfs(argv: Vec<&str>, vfs: Arc<dyn Vfs>) -> (String, i32) {
        let err = tempfile::NamedTempFile::new().unwrap();
        let ctx = ProcessContext {
            argv: argv.into_iter().map(str::to_string).collect(),
            env: HashMap::new(),
            io: ProcessIo {
                stdin: OpenFile::Stdin(std::io::stdin()),
                stdout: OpenFile::Stdout(std::io::stdout()),
                stderr: OpenFile::from(err.reopen().unwrap()),
            },
            pid: 0,
            cwd: std::path::PathBuf::from("/"),
        };
        let result = MkdirProcess::new(vfs).run(ctx).await;
        (
            std::fs::read_to_string(err.path()).unwrap(),
            result.exit_code,
        )
    }

    #[tokio::test]
    async fn test_mkdir_missing_operand_exits_1() {
        let vfs = Arc::new(MockVfs::new());
        let (stderr, code) = run_with_vfs(vec!["mkdir"], vfs).await;
        assert_eq!(code, 1);
        assert!(stderr.contains("missing operand"), "got: {stderr}");
    }

    #[tokio::test]
    async fn test_mkdir_creates_directory() {
        let dir = tempfile::tempdir().unwrap();
        let new_dir = dir.path().join("newdir");
        let vfs = Arc::new(clank_vfs::RealFs);
        let (_, code) = run_with_vfs(vec!["mkdir", new_dir.to_str().unwrap()], vfs).await;
        assert_eq!(code, 0);
        assert!(new_dir.is_dir(), "directory was not created");
    }

    #[tokio::test]
    async fn test_mkdir_p_creates_nested_directories() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("a").join("b").join("c");
        let vfs = Arc::new(clank_vfs::RealFs);
        let (_, code) = run_with_vfs(vec!["mkdir", "-p", nested.to_str().unwrap()], vfs).await;
        assert_eq!(code, 0);
        assert!(nested.is_dir(), "nested directory was not created");
    }

    #[tokio::test]
    async fn test_mkdir_combined_flag_p() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("x").join("y");
        let vfs = Arc::new(clank_vfs::RealFs);
        // -p must be recognized even when combined with another char.
        let (_, code) = run_with_vfs(vec!["mkdir", "-vp", nested.to_str().unwrap()], vfs).await;
        assert_eq!(code, 0);
        assert!(nested.is_dir());
    }
}
