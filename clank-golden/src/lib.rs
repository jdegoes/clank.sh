use std::io::{self, Read};
use std::path::{Path, PathBuf};

use serde::Deserialize;

// ── YAML schema ──────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct GoldenTest {
    pub description: String,
    /// Relative path to another YAML file whose script is run first.
    pub setup: Option<String>,
    /// Shell input lines to execute.
    pub script: String,
    /// Expected exact stdout. Omit to skip assertion.
    pub stdout: Option<String>,
    /// Expected exact stderr. Omit to skip assertion.
    pub stderr: Option<String>,
    /// If present, this is a complete test: send EOF, wait for exit, assert code.
    pub exit_code: Option<i32>,
}

/// Parse a golden test from embedded YAML content.
/// `base_dir` is the directory the YAML file lives in (used to resolve `setup`).
pub fn load(content: &str) -> GoldenTest {
    serde_yaml::from_str(content).expect("failed to parse golden test YAML")
}

// ── Setup chain resolution ────────────────────────────────────────────────────

/// Recursively resolve the chain of setup scripts for a test.
/// Returns scripts in order from outermost setup to innermost, before the test
/// script itself. Cycle detection is via the visited path set.
pub fn resolve_setup_chain(test: &GoldenTest, base_dir: &Path) -> Vec<String> {
    let mut scripts = Vec::new();
    collect_setup_scripts(test, base_dir, &mut scripts, &mut Vec::new());
    scripts
}

fn collect_setup_scripts(
    test: &GoldenTest,
    base_dir: &Path,
    scripts: &mut Vec<String>,
    visited: &mut Vec<PathBuf>,
) {
    let Some(ref setup_rel) = test.setup else {
        return;
    };

    let setup_path = base_dir.join(setup_rel);
    let canonical = setup_path
        .canonicalize()
        .unwrap_or_else(|_| setup_path.clone());

    if visited.contains(&canonical) {
        panic!(
            "golden test setup cycle detected: {}",
            canonical.display()
        );
    }
    visited.push(canonical.clone());

    let content = std::fs::read_to_string(&setup_path).unwrap_or_else(|e| {
        panic!(
            "failed to read setup file {}: {e}",
            setup_path.display()
        )
    });
    let setup_test: GoldenTest = serde_yaml::from_str(&content).unwrap_or_else(|e| {
        panic!(
            "failed to parse setup file {}: {e}",
            setup_path.display()
        )
    });

    let setup_dir = setup_path.parent().unwrap_or(base_dir);
    collect_setup_scripts(&setup_test, setup_dir, scripts, visited);
    scripts.push(setup_test.script);
}

// ── Runner ────────────────────────────────────────────────────────────────────

/// Run a golden test. Panics with a descriptive message on failure.
/// Call this from generated `#[test]` functions.
pub fn run_embedded(yaml_content: &str, base_dir: &str) {
    let test = load(yaml_content);
    let base = Path::new(base_dir);
    run(&test, base);
}

pub fn run(test: &GoldenTest, base_dir: &Path) {
    let setup_scripts = resolve_setup_chain(test, base_dir);

    // Build the tokio runtime — golden tests are sync `#[test]` fns.
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("failed to build tokio runtime");

    rt.block_on(async {
        let mut shell = clank::build_shell().await;

        // Run setup scripts in the same shell instance (no output capture needed).
        for script in &setup_scripts {
            run_script_no_capture(&mut shell, script).await;
        }

        // Run the test script with output capture.
        let (actual_stdout, actual_stderr) =
            run_script_captured(&mut shell, &test.script).await;

        // Assert stdout if specified.
        if let Some(expected) = &test.stdout {
            assert_output_eq("stdout", expected, &actual_stdout);
        }

        // Assert stderr if specified.
        if let Some(expected) = &test.stderr {
            assert_output_eq("stderr", expected, &actual_stderr);
        }

        // For complete tests, assert exit code.
        if let Some(expected_code) = test.exit_code {
            let actual_code = shell.last_result() as i32;
            assert_eq!(
                actual_code,
                expected_code,
                "exit code mismatch in '{}': expected {expected_code}, got {actual_code}",
                test.description
            );
        }
    });
}

// ── Internal helpers ──────────────────────────────────────────────────────────

async fn run_script_no_capture(shell: &mut clank::ClankShell, script: &str) {
    let params = shell.default_exec_params();
    for line in script.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let _ = shell.run_string_raw(trimmed, &params).await;
    }
}

async fn run_script_captured(
    shell: &mut clank::ClankShell,
    script: &str,
) -> (String, String) {
    use brush_core::openfiles::OpenFile;

    // Create pipes for stdout and stderr capture.
    let (mut stdout_reader, stdout_writer) =
        io::pipe().expect("failed to create stdout pipe");
    let (mut stderr_reader, stderr_writer) =
        io::pipe().expect("failed to create stderr pipe");

    let mut params = shell.default_exec_params();
    params.set_fd(1, OpenFile::PipeWriter(stdout_writer));
    params.set_fd(2, OpenFile::PipeWriter(stderr_writer));

    for line in script.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let _ = shell.run_string_raw(trimmed, &params).await;
    }

    // Drop params to close the write ends of the pipes before reading.
    drop(params);

    let mut stdout_buf = String::new();
    stdout_reader
        .read_to_string(&mut stdout_buf)
        .expect("failed to read stdout pipe");

    let mut stderr_buf = String::new();
    stderr_reader
        .read_to_string(&mut stderr_buf)
        .expect("failed to read stderr pipe");

    (stdout_buf, stderr_buf)
}

fn assert_output_eq(stream: &str, expected: &str, actual: &str) {
    if expected == actual {
        return;
    }
    // Produce a simple line-by-line diff message.
    let mut msg = format!(
        "{stream} mismatch:\n  expected: {expected:?}\n  actual:   {actual:?}\n\ndiff:\n"
    );
    let exp_lines: Vec<&str> = expected.lines().collect();
    let act_lines: Vec<&str> = actual.lines().collect();
    let max = exp_lines.len().max(act_lines.len());
    for i in 0..max {
        let e = exp_lines.get(i).copied().unwrap_or("<missing>");
        let a = act_lines.get(i).copied().unwrap_or("<missing>");
        if e != a {
            msg.push_str(&format!("  line {}: expected {e:?}, got {a:?}\n", i + 1));
        }
    }
    panic!("{msg}");
}
