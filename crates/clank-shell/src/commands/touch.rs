use std::sync::Arc;

use async_trait::async_trait;
use clank_vfs::Vfs;

use crate::commands::resolve;
use crate::process::{Process, ProcessContext, ProcessResult};

pub struct TouchProcess {
    vfs: Arc<dyn Vfs>,
}

impl TouchProcess {
    pub fn new(vfs: Arc<dyn Vfs>) -> Self {
        Self { vfs }
    }
}

#[async_trait]
impl Process for TouchProcess {
    async fn run(&self, mut ctx: ProcessContext) -> ProcessResult {
        let paths: Vec<String> = ctx
            .argv
            .iter()
            .skip(1)
            .filter(|a| !a.starts_with('-'))
            .cloned()
            .collect();

        if paths.is_empty() {
            let _ = ctx.io.write_stderr(b"touch: missing file operand\n");
            return ProcessResult::failure(1);
        }

        let mut exit_code = 0;
        for path in &paths {
            let resolved = resolve(&ctx.cwd, path);
            // For existing files, write_file updates content (effectively touching).
            // For new files, write_file creates them. Both paths go through the VFS.
            let result = if self.vfs.exists(&resolved) {
                // Preserve content — write back the existing bytes unchanged.
                match self.vfs.read_file(&resolved) {
                    Ok(existing) => self.vfs.write_file(&resolved, &existing),
                    Err(e) => Err(e),
                }
            } else {
                self.vfs.write_file(&resolved, b"")
            };
            if let Err(e) = result {
                let _ = ctx
                    .io
                    .write_stderr(format!("touch: {path}: {e}\n").as_bytes());
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
        let result = TouchProcess::new(vfs).run(ctx).await;
        (
            std::fs::read_to_string(err.path()).unwrap(),
            result.exit_code,
        )
    }

    #[tokio::test]
    async fn test_touch_missing_operand_exits_1() {
        let vfs = Arc::new(MockVfs::new());
        let (stderr, code) = run_with_vfs(vec!["touch"], Arc::clone(&vfs) as Arc<dyn Vfs>).await;
        assert_eq!(code, 1);
        assert!(stderr.contains("missing file operand"), "got: {stderr}");
    }

    #[tokio::test]
    async fn test_touch_creates_new_file_via_vfs() {
        let vfs = Arc::new(MockVfs::new());
        let (_, code) = run_with_vfs(
            vec!["touch", "/new/file.txt"],
            Arc::clone(&vfs) as Arc<dyn Vfs>,
        )
        .await;
        assert_eq!(code, 0);
        use clank_vfs::Vfs as _;
        assert!(
            vfs.exists(std::path::Path::new("/new/file.txt")),
            "file must exist in vfs after touch"
        );
    }

    #[tokio::test]
    async fn test_touch_existing_file_preserves_content() {
        let vfs = Arc::new(MockVfs::new().with_file("/existing.txt", "original content"));
        let (_, code) = run_with_vfs(
            vec!["touch", "/existing.txt"],
            Arc::clone(&vfs) as Arc<dyn Vfs>,
        )
        .await;
        assert_eq!(code, 0);
        use clank_vfs::Vfs as _;
        let content = vfs
            .read_file(std::path::Path::new("/existing.txt"))
            .unwrap();
        assert_eq!(content, b"original content");
    }

    #[tokio::test]
    async fn test_touch_real_filesystem_creates_file() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("new.txt");
        let vfs = Arc::new(clank_vfs::RealFs);
        let (_, code) = run_with_vfs(vec!["touch", file.to_str().unwrap()], vfs).await;
        assert_eq!(code, 0);
        assert!(file.exists(), "file must be created on real fs");
    }
}
