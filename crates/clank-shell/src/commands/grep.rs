use std::fmt::Write as _;
use std::sync::Arc;

use async_trait::async_trait;
use clank_vfs::Vfs;

use crate::commands::resolve;
use crate::process::{Process, ProcessContext, ProcessResult};

pub struct GrepProcess {
    vfs: Arc<dyn Vfs>,
}

impl GrepProcess {
    pub fn new(vfs: Arc<dyn Vfs>) -> Self {
        Self { vfs }
    }
}

#[async_trait]
impl Process for GrepProcess {
    async fn run(&self, mut ctx: ProcessContext) -> ProcessResult {
        // Parse flags using has_flag so combined forms (e.g. -rn, -ni) work.
        let recursive =
            has_flag(&ctx.argv, 'r', Some("--recursive")) || has_flag(&ctx.argv, 'R', None);
        let show_line_numbers = has_flag(&ctx.argv, 'n', Some("--line-number"));
        let files_only = has_flag(&ctx.argv, 'l', Some("--files-with-matches"));
        let ignore_case = has_flag(&ctx.argv, 'i', Some("--ignore-case"));

        let positional: Vec<String> = ctx
            .argv
            .iter()
            .skip(1)
            .filter(|a| !a.starts_with('-'))
            .cloned()
            .collect();

        if positional.is_empty() {
            let _ = ctx.io.write_stderr(b"grep: missing pattern\n");
            return ProcessResult::failure(2);
        }

        let pattern = positional[0].clone();
        let paths: Vec<String> = positional[1..].to_vec();

        let mut found = false;
        let mut exit_code = 1;

        if paths.is_empty() {
            use std::io::Read;
            let mut buf = String::new();
            let _ = ctx.io.stdin.read_to_string(&mut buf);
            found |= grep_content(
                &buf,
                &pattern,
                None,
                show_line_numbers,
                files_only,
                ignore_case,
                &mut ctx,
            );
        } else {
            for path in &paths {
                let resolved = resolve(&ctx.cwd, path);
                if recursive {
                    found |= grep_recursive(
                        self.vfs.as_ref(),
                        &resolved,
                        &pattern,
                        show_line_numbers,
                        files_only,
                        ignore_case,
                        &mut ctx,
                    );
                } else {
                    match self.vfs.read_file(&resolved) {
                        Ok(bytes) => {
                            let content = String::from_utf8_lossy(&bytes).into_owned();
                            found |= grep_content(
                                &content,
                                &pattern,
                                Some(path.as_str()),
                                show_line_numbers,
                                files_only,
                                ignore_case,
                                &mut ctx,
                            );
                        }
                        Err(e) => {
                            let _ = ctx
                                .io
                                .write_stderr(format!("grep: {path}: {e}\n").as_bytes());
                            exit_code = 2;
                        }
                    }
                }
            }
        }

        if found {
            exit_code = 0;
        }
        if exit_code == 0 {
            ProcessResult::success()
        } else {
            ProcessResult::failure(exit_code)
        }
    }
}

/// Check whether a flag is present in argv, supporting combined short flags
/// (e.g. `-rn`, `-ni`) as well as standalone (`-r`) and long forms.
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

fn grep_content(
    content: &str,
    pattern: &str,
    filename: Option<&str>,
    show_line_numbers: bool,
    files_only: bool,
    ignore_case: bool,
    ctx: &mut ProcessContext,
) -> bool {
    // Compute the lowercased pattern once, not once per line (Q5).
    let pattern_lower = ignore_case.then(|| pattern.to_lowercase());

    let mut found = false;
    for (i, line) in content.lines().enumerate() {
        let matches = match &pattern_lower {
            Some(p) => line.to_lowercase().contains(p.as_str()),
            None => line.contains(pattern),
        };
        if matches {
            found = true;
            if files_only {
                if let Some(f) = filename {
                    let _ = ctx.io.write_stdout(format!("{f}\n").as_bytes());
                }
                return found;
            }
            let mut out = String::new();
            if let Some(f) = filename {
                out.push_str(f);
                out.push(':');
            }
            if show_line_numbers {
                let _ = write!(out, "{}:", i + 1);
            }
            out.push_str(line);
            out.push('\n');
            let _ = ctx.io.write_stdout(out.as_bytes());
        }
    }
    found
}

// Q6: takes &dyn Vfs instead of &Arc<dyn Vfs> — no need to clone the Arc.
fn grep_recursive(
    vfs: &dyn Vfs,
    path: &std::path::Path,
    pattern: &str,
    show_line_numbers: bool,
    files_only: bool,
    ignore_case: bool,
    ctx: &mut ProcessContext,
) -> bool {
    let mut found = false;
    match vfs.stat(path) {
        Ok(stat) if stat.is_dir => {
            if let Ok(entries) = vfs.read_dir(path) {
                for entry in entries {
                    found |= grep_recursive(
                        vfs,
                        &entry.path,
                        pattern,
                        show_line_numbers,
                        files_only,
                        ignore_case,
                        ctx,
                    );
                }
            }
        }
        Ok(_) => {
            if let Ok(bytes) = vfs.read_file(path) {
                let content = String::from_utf8_lossy(&bytes).into_owned();
                found |= grep_content(
                    &content,
                    pattern,
                    path.to_str(),
                    show_line_numbers,
                    files_only,
                    ignore_case,
                    ctx,
                );
            }
        }
        _ => {}
    }
    found
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

    async fn run(argv: Vec<&str>, vfs: Arc<dyn Vfs>) -> (String, String, i32) {
        let out = tempfile::NamedTempFile::new().unwrap();
        let err = tempfile::NamedTempFile::new().unwrap();
        let ctx = make_ctx(argv, out.reopen().unwrap(), err.reopen().unwrap());
        let result = GrepProcess::new(vfs).run(ctx).await;
        (
            std::fs::read_to_string(out.path()).unwrap(),
            std::fs::read_to_string(err.path()).unwrap(),
            result.exit_code,
        )
    }

    #[tokio::test]
    async fn test_grep_match_found_exits_0() {
        let vfs = Arc::new(MockVfs::new().with_file("/f", "hello world\ngoodbye\n"));
        let (stdout, _, code) = run(vec!["grep", "hello", "/f"], vfs).await;
        assert_eq!(code, 0);
        assert!(
            stdout.contains("hello world"),
            "matched line missing: {stdout}"
        );
    }

    #[tokio::test]
    async fn test_grep_no_match_exits_1() {
        let vfs = Arc::new(MockVfs::new().with_file("/f", "hello world\n"));
        let (stdout, _, code) = run(vec!["grep", "zzz", "/f"], vfs).await;
        assert_eq!(code, 1);
        assert!(stdout.is_empty(), "stdout must be empty when no match");
    }

    #[tokio::test]
    async fn test_grep_missing_pattern_exits_2() {
        let vfs = Arc::new(MockVfs::new());
        let (_, stderr, code) = run(vec!["grep"], vfs).await;
        assert_eq!(code, 2);
        assert!(stderr.contains("missing pattern"), "got: {stderr}");
    }

    #[tokio::test]
    async fn test_grep_case_insensitive_flag() {
        let vfs = Arc::new(MockVfs::new().with_file("/f", "Hello World\n"));
        let (stdout, _, code) = run(vec!["grep", "-i", "hello", "/f"], vfs).await;
        assert_eq!(code, 0);
        assert!(
            stdout.contains("Hello World"),
            "matched line missing: {stdout}"
        );
    }

    #[tokio::test]
    async fn test_grep_line_numbers_flag() {
        let vfs = Arc::new(MockVfs::new().with_file("/f", "skip\nmatch\nskip\n"));
        let (stdout, _, code) = run(vec!["grep", "-n", "match", "/f"], vfs).await;
        assert_eq!(code, 0);
        assert!(stdout.contains("2:match"), "line number missing: {stdout}");
    }

    #[tokio::test]
    async fn test_grep_files_only_flag() {
        let vfs = Arc::new(MockVfs::new().with_file("/f", "found here\n"));
        let (stdout, _, code) = run(vec!["grep", "-l", "found", "/f"], vfs).await;
        assert_eq!(code, 0);
        assert!(stdout.contains("/f"), "filename missing: {stdout}");
        assert!(
            !stdout.contains("found here"),
            "line must not appear with -l"
        );
    }

    #[tokio::test]
    async fn test_grep_missing_file_exits_2_with_error() {
        let vfs = Arc::new(MockVfs::new());
        let (_, stderr, code) = run(vec!["grep", "pattern", "/no/such"], vfs).await;
        assert_eq!(code, 2);
        assert!(stderr.contains("grep:"), "error prefix missing: {stderr}");
        assert!(stderr.contains("/no/such"), "path missing: {stderr}");
    }

    #[tokio::test]
    async fn test_grep_recursive_finds_in_subdirectory() {
        let vfs = Arc::new(
            MockVfs::new()
                .with_file("/src/main.rs", "fn main() { }\n")
                .with_file("/src/lib.rs", "pub fn helper() { }\n"),
        );
        let (stdout, _, code) = run(vec!["grep", "-r", "fn main", "/src"], vfs).await;
        assert_eq!(code, 0);
        assert!(stdout.contains("main.rs"), "matched file missing: {stdout}");
    }

    #[tokio::test]
    async fn test_grep_combined_flags_rn() {
        let vfs = Arc::new(MockVfs::new().with_file("/src/f.rs", "skip\nmatch\n"));
        let (stdout, _, code) = run(vec!["grep", "-rn", "match", "/src"], vfs).await;
        assert_eq!(code, 0);
        assert!(
            stdout.contains("2:match"),
            "combined -rn must apply both recursive and line-number: {stdout}"
        );
    }

    #[tokio::test]
    async fn test_grep_combined_flags_ni() {
        let vfs = Arc::new(MockVfs::new().with_file("/f", "skip\nHello\n"));
        let (stdout, _, code) = run(vec!["grep", "-ni", "hello", "/f"], vfs).await;
        assert_eq!(code, 0);
        assert!(
            stdout.contains("2:Hello"),
            "combined -ni must show line numbers with case-insensitive match: {stdout}"
        );
    }
}
