use std::sync::Arc;

use async_trait::async_trait;
use clank_vfs::Vfs;

use crate::commands::resolve;
use crate::process::{Process, ProcessContext, ProcessResult};

pub struct RmProcess {
    vfs: Arc<dyn Vfs>,
}

impl RmProcess {
    pub fn new(vfs: Arc<dyn Vfs>) -> Self {
        Self { vfs }
    }
}

#[async_trait]
impl Process for RmProcess {
    async fn run(&self, mut ctx: ProcessContext) -> ProcessResult {
        let recursive =
            has_flag(&ctx.argv, 'r', Some("--recursive")) || has_flag(&ctx.argv, 'R', None);
        let force = has_flag(&ctx.argv, 'f', Some("--force"));

        let paths: Vec<String> = ctx
            .argv
            .iter()
            .skip(1)
            .filter(|a| !a.starts_with('-'))
            .cloned()
            .collect();

        if paths.is_empty() {
            let _ = ctx.io.write_stderr(b"rm: missing operand\n");
            return ProcessResult::failure(1);
        }

        let mut exit_code = 0;
        for path in &paths {
            let resolved = resolve(&ctx.cwd, path);
            let result = if recursive {
                match self.vfs.stat(&resolved) {
                    Ok(s) if s.is_dir => self.vfs.remove_dir_all(&resolved),
                    Ok(_) => self.vfs.remove_file(&resolved),
                    Err(_) if force => Ok(()),
                    Err(e) => Err(e),
                }
            } else {
                self.vfs.remove_file(&resolved)
            };
            if let Err(e) = result {
                if !force {
                    let _ = ctx.io.write_stderr(format!("rm: {path}: {e}\n").as_bytes());
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

/// Check whether a flag is present in argv, supporting combined short flags
/// (e.g. `-rf`) as well as standalone (`-r`) and long (`--recursive`).
fn has_flag(argv: &[String], short: char, long: Option<&str>) -> bool {
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
        let result = RmProcess::new(vfs).run(ctx).await;
        (
            std::fs::read_to_string(err.path()).unwrap(),
            result.exit_code,
        )
    }

    #[tokio::test]
    async fn test_rm_missing_operand_exits_1() {
        let vfs = Arc::new(MockVfs::new());
        let (stderr, code) = run_with_vfs(vec!["rm"], vfs).await;
        assert_eq!(code, 1);
        assert!(stderr.contains("missing operand"), "got: {stderr}");
    }

    #[tokio::test]
    async fn test_rm_removes_file_via_vfs() {
        let vfs = Arc::new(MockVfs::new().with_file("/target.txt", "data"));
        let (_, code) =
            run_with_vfs(vec!["rm", "/target.txt"], Arc::clone(&vfs) as Arc<dyn Vfs>).await;
        assert_eq!(code, 0);
        use clank_vfs::Vfs as _;
        assert!(
            !vfs.exists(std::path::Path::new("/target.txt")),
            "file must be removed from vfs"
        );
    }

    #[tokio::test]
    async fn test_rm_r_removes_directory_via_vfs() {
        let vfs = Arc::new(
            MockVfs::new()
                .with_file("/dir/file1.txt", "a")
                .with_file("/dir/file2.txt", "b"),
        );
        let (_, code) =
            run_with_vfs(vec!["rm", "-r", "/dir"], Arc::clone(&vfs) as Arc<dyn Vfs>).await;
        assert_eq!(code, 0);
        use clank_vfs::Vfs as _;
        assert!(
            !vfs.exists(std::path::Path::new("/dir/file1.txt")),
            "files in dir must be removed"
        );
    }

    #[tokio::test]
    async fn test_rm_f_suppresses_error_for_missing_file() {
        let vfs = Arc::new(MockVfs::new());
        let (stderr, code) = run_with_vfs(vec!["rm", "-f", "/no/such/file"], vfs).await;
        assert_eq!(code, 0, "-f must suppress error for missing file");
        assert!(stderr.is_empty(), "stderr must be empty with -f: {stderr}");
    }

    #[tokio::test]
    async fn test_rm_combined_rf_flag() {
        let vfs = Arc::new(MockVfs::new().with_file("/dir/file.txt", "x"));
        let (stderr, code) =
            run_with_vfs(vec!["rm", "-rf", "/dir"], Arc::clone(&vfs) as Arc<dyn Vfs>).await;
        assert_eq!(code, 0, "rm -rf must exit 0; stderr: {stderr}");
        use clank_vfs::Vfs as _;
        assert!(!vfs.exists(std::path::Path::new("/dir/file.txt")));
    }

    #[tokio::test]
    async fn test_rm_real_filesystem() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("target.txt");
        std::fs::write(&file, "data").unwrap();
        let vfs = Arc::new(clank_vfs::RealFs);
        let (_, code) = run_with_vfs(vec!["rm", file.to_str().unwrap()], vfs).await;
        assert_eq!(code, 0);
        assert!(!file.exists(), "file must be removed");
    }
}
