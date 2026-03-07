use std::fmt::Write as _;
use std::sync::Arc;

use async_trait::async_trait;
use clank_vfs::Vfs;

use crate::commands::resolve;
use crate::process::{Process, ProcessContext, ProcessResult};

pub struct LsProcess {
    vfs: Arc<dyn Vfs>,
}

impl LsProcess {
    pub fn new(vfs: Arc<dyn Vfs>) -> Self {
        Self { vfs }
    }
}

#[async_trait]
impl Process for LsProcess {
    async fn run(&self, mut ctx: ProcessContext) -> ProcessResult {
        // Use has_flag so combined forms (-la, -al) work correctly.
        let show_all = has_flag(&ctx.argv, 'a', Some("--all"));
        let long = has_flag(&ctx.argv, 'l', None);

        let paths: Vec<String> = ctx
            .argv
            .iter()
            .skip(1)
            .filter(|a| !a.starts_with('-'))
            .cloned()
            .collect();

        let targets = if paths.is_empty() {
            vec![".".to_string()]
        } else {
            paths
        };

        let mut exit_code = 0;
        for target in &targets {
            let path = resolve(&ctx.cwd, target);
            match self.vfs.stat(&path) {
                Ok(stat) if stat.is_dir => match self.vfs.read_dir(&path) {
                    Ok(entries) => {
                        let mut names: Vec<_> = entries
                            .iter()
                            .filter(|e| {
                                show_all
                                    || !e
                                        .path
                                        .file_name()
                                        .and_then(|n| n.to_str())
                                        .unwrap_or("")
                                        .starts_with('.')
                            })
                            .map(|e| {
                                let name = e
                                    .path
                                    .file_name()
                                    .and_then(|n| n.to_str())
                                    .unwrap_or("")
                                    .to_string();
                                let suffix = if e.is_dir { "/" } else { "" };
                                (name, suffix, e.is_dir)
                            })
                            .collect();
                        names.sort_by(|a, b| a.0.cmp(&b.0));

                        let mut out = String::new();
                        if long {
                            for (name, suffix, is_dir) in &names {
                                let kind = if *is_dir { "d" } else { "-" };
                                let _ = writeln!(out, "{kind}rwxr-xr-x  {name}{suffix}");
                            }
                        } else {
                            let line: Vec<_> =
                                names.iter().map(|(n, s, _)| format!("{n}{s}")).collect();
                            out.push_str(&line.join("  "));
                            out.push('\n');
                        }
                        let _ = ctx.io.write_stdout(out.as_bytes());
                    }
                    Err(e) => {
                        let _ = ctx
                            .io
                            .write_stderr(format!("ls: {target}: {e}\n").as_bytes());
                        exit_code = 1;
                    }
                },
                Ok(_) => {
                    let _ = ctx.io.write_stdout(format!("{target}\n").as_bytes());
                }
                Err(e) => {
                    let _ = ctx
                        .io
                        .write_stderr(format!("ls: {target}: {e}\n").as_bytes());
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
        let result = LsProcess::new(vfs).run(ctx).await;
        (
            std::fs::read_to_string(out.path()).unwrap(),
            std::fs::read_to_string(err.path()).unwrap(),
            result.exit_code,
        )
    }

    #[tokio::test]
    async fn test_ls_directory_shows_contents() {
        let vfs = Arc::new(
            MockVfs::new()
                .with_file("/dir/alpha.txt", "")
                .with_file("/dir/beta.txt", ""),
        );
        let (stdout, _, code) = run(vec!["ls", "/dir"], vfs).await;
        assert_eq!(code, 0);
        assert!(stdout.contains("alpha.txt"), "alpha.txt missing: {stdout}");
        assert!(stdout.contains("beta.txt"), "beta.txt missing: {stdout}");
    }

    #[tokio::test]
    async fn test_ls_hidden_files_filtered_by_default() {
        let vfs = Arc::new(
            MockVfs::new()
                .with_file("/dir/visible.txt", "")
                .with_file("/dir/.hidden", ""),
        );
        let (stdout, _, code) = run(vec!["ls", "/dir"], vfs).await;
        assert_eq!(code, 0);
        assert!(
            stdout.contains("visible.txt"),
            "visible file missing: {stdout}"
        );
        assert!(
            !stdout.contains(".hidden"),
            "hidden file must not appear without -a"
        );
    }

    #[tokio::test]
    async fn test_ls_dash_a_shows_hidden_files() {
        let vfs = Arc::new(
            MockVfs::new()
                .with_file("/dir/visible.txt", "")
                .with_file("/dir/.hidden", ""),
        );
        let (stdout, _, code) = run(vec!["ls", "-a", "/dir"], vfs).await;
        assert_eq!(code, 0);
        assert!(
            stdout.contains(".hidden"),
            "hidden file must appear with -a: {stdout}"
        );
    }

    #[tokio::test]
    async fn test_ls_long_format() {
        let vfs = Arc::new(MockVfs::new().with_file("/dir/file.txt", "content"));
        let (stdout, _, code) = run(vec!["ls", "-l", "/dir"], vfs).await;
        assert_eq!(code, 0);
        assert!(
            stdout.contains("-rwxr-xr-x"),
            "long format permission string missing: {stdout}"
        );
    }

    #[tokio::test]
    async fn test_ls_single_file_prints_name() {
        let vfs = Arc::new(MockVfs::new().with_file("/dir/file.txt", "x"));
        let (stdout, _, code) = run(vec!["ls", "/dir/file.txt"], vfs).await;
        assert_eq!(code, 0);
        assert!(
            stdout.contains("/dir/file.txt"),
            "file path missing: {stdout}"
        );
    }

    #[tokio::test]
    async fn test_ls_missing_path_exits_1_with_error() {
        let vfs = Arc::new(MockVfs::new());
        let (stdout, stderr, code) = run(vec!["ls", "/no/such"], vfs).await;
        assert_eq!(code, 1);
        assert!(stderr.contains("ls:"), "error prefix missing: {stderr}");
        assert!(
            stderr.contains("/no/such"),
            "path missing from error: {stderr}"
        );
        assert!(stdout.is_empty(), "stdout must be empty on error");
    }

    #[tokio::test]
    async fn test_ls_combined_la_shows_hidden_long_format() {
        let vfs = Arc::new(
            MockVfs::new()
                .with_file("/dir/visible.txt", "")
                .with_file("/dir/.hidden", ""),
        );
        let (stdout, _, code) = run(vec!["ls", "-la", "/dir"], vfs).await;
        assert_eq!(code, 0);
        assert!(
            stdout.contains(".hidden"),
            "combined -la must show hidden files"
        );
        assert!(
            stdout.contains("-rwxr-xr-x"),
            "combined -la must show long format"
        );
    }
}
