//! Acceptance test harness for clank.
//!
//! This file is the entry point for the `clank-acceptance` test binary. It
//! uses [`datatest_stable`] to discover every `.yaml` file under `cases/`,
//! deserialise it as a [`TestSuite`], and run each [`TestCase`] against the
//! compiled `clank` binary.
//!
//! ## Adding test cases
//!
//! Drop a new `.yaml` file anywhere under `clank-acceptance/cases/`. No code
//! changes required — the harness discovers it automatically.
//!
//! ## Test case schema
//!
//! See the [`TestCase`] struct for full field documentation.

use std::collections::HashMap;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde::Deserialize;

// ---------------------------------------------------------------------------
// Binary path resolution.
//
// We resolve the path at test runtime rather than compile time, avoiding the
// need for a build script or generated files.
//
// Strategy:
//   1. Respect CARGO_TARGET_DIR if set (handles custom target directories).
//   2. Otherwise derive the target directory from CARGO_MANIFEST_DIR — which
//      always points to the clank-acceptance package root — by stepping up
//      one level to the workspace root and appending "target/".
//   3. Within the target directory, prefer the debug profile build. Release
//      profile support is a known limitation: when running `cargo test
//      --release`, the binary at target/debug/clank will still be used unless
//      CARGO_TARGET_DIR is set to point elsewhere. This is acceptable until
//      release-mode CI testing becomes a requirement.
//
// The assert! gives a clear error message at test startup rather than a
// cryptic spawn failure later.
// ---------------------------------------------------------------------------

/// Returns the path to the compiled `clank` binary.
fn clank_binary() -> PathBuf {
    let target_dir = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            // CARGO_MANIFEST_DIR is clank-acceptance/; parent is workspace root.
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .expect("clank-acceptance has no parent directory")
                .join("target")
        });

    let bin = target_dir.join("debug").join("clank");

    assert!(
        bin.exists(),
        "clank binary not found at {bin:?} — run `cargo build -p clank-shell` first"
    );

    bin
}

// ---------------------------------------------------------------------------
// Schema
// ---------------------------------------------------------------------------

/// Top-level structure of each `.yaml` test file.
#[derive(Debug, Deserialize)]
struct TestSuite {
    /// Human-readable name for the suite (used in failure messages).
    name: String,
    /// The individual test cases in this suite.
    cases: Vec<TestCase>,
}

/// A single acceptance test case.
#[derive(Debug, Deserialize)]
struct TestCase {
    /// Short description — used as the test identifier in failure output.
    name: String,

    /// Shell script source. Passed to `clank` via stdin.
    stdin: String,

    /// Extra command-line arguments passed to `clank` (default: none).
    #[serde(default)]
    args: Vec<String>,

    /// Extra environment variables set for the `clank` process.
    #[serde(default)]
    env: HashMap<String, String>,

    /// Expected process exit code (default: 0).
    #[serde(default)]
    expect_exit: u8,

    /// Expected exact stdout content, including any trailing newline.
    /// If absent, stdout is not checked.
    expect_stdout: Option<String>,

    /// A substring that must appear somewhere in stdout.
    /// If absent, stdout is not checked for containment.
    expect_stdout_contains: Option<String>,

    /// A list of substrings that must all appear somewhere in stdout.
    /// Every entry in the list is checked independently; all must match.
    /// Use this to assert multiple distinct strings in a single test case
    /// without needing to duplicate the stdin script.
    #[serde(default)]
    expect_stdout_contains_all: Vec<String>,

    /// A list of glob-style patterns where each pattern must match at least
    /// one complete line in stdout. `*` in a pattern matches any sequence of
    /// characters within a line (it does not cross newline boundaries).
    ///
    /// Example — assert a timestamped transcript line:
    ///
    ///   expect_stdout_line_matches:
    ///     - "[*] command: echo hello"
    ///     - "[*] output: hello"
    #[serde(default)]
    expect_stdout_line_matches: Vec<String>,

    /// If `true`, assert that stderr is completely empty.
    /// If `false` (the default), stderr is not checked.
    #[serde(default)]
    expect_stderr_empty: bool,

    /// If `true`, the test is executed but a failure is not propagated as a
    /// hard test failure. Documents known divergences without silencing them.
    #[serde(default)]
    known_failure: bool,

    /// If `true`, the test is skipped entirely (not executed).
    #[serde(default)]
    skip: bool,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Returns `true` if `line` matches `pattern`, where `*` in the pattern
/// matches any sequence of characters (but does not cross line boundaries).
///
/// Implemented by splitting the pattern on `*` and checking that the
/// resulting segments appear in order within the line.
fn glob_match(pattern: &str, line: &str) -> bool {
    let mut segments = pattern.split('*');
    let first = segments.next().unwrap_or("");
    if !line.starts_with(first) {
        return false;
    }
    let mut pos = first.len();
    for segment in segments {
        if segment.is_empty() {
            // A trailing `*` or consecutive `**` — always matches remaining.
            continue;
        }
        match line[pos..].find(segment) {
            Some(offset) => pos += offset + segment.len(),
            None => return false,
        }
    }
    true
}

// ---------------------------------------------------------------------------
// Runner
// ---------------------------------------------------------------------------

/// Entry point called by `datatest_stable` once per discovered `.yaml` file.
fn run_suite(path: &Path) -> datatest_stable::Result<()> {
    let contents = std::fs::read_to_string(path)
        .map_err(|e| format!("failed to read {}: {e}", path.display()))?;

    let suite: TestSuite = serde_yaml_ng::from_str(&contents)
        .map_err(|e| format!("failed to parse {}: {e}", path.display()))?;

    let mut failures: Vec<String> = Vec::new();

    for case in &suite.cases {
        if case.skip {
            continue;
        }

        match run_case(&suite.name, case) {
            Ok(()) => {}
            Err(msg) => {
                if case.known_failure {
                    // Known failure: log but do not propagate.
                    eprintln!("[known_failure] {msg}");
                } else {
                    failures.push(msg);
                }
            }
        }
    }

    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n\n").into())
    }
}

/// Run a single test case, returning `Ok(())` on success or an error string.
fn run_case(suite_name: &str, case: &TestCase) -> Result<(), String> {
    let bin = clank_binary();
    let mut child = Command::new(&bin)
        .args(&case.args)
        .envs(&case.env)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            format!(
                "[{suite_name}] \"{}\": failed to spawn clank ({bin:?}): {e}",
                case.name
            )
        })?;

    // Write the script to stdin and close it so the process sees EOF.
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(case.stdin.as_bytes())
            .map_err(|e| format!("[{suite_name}] \"{}\": stdin write failed: {e}", case.name))?;
        // stdin is dropped here, closing the pipe
    }

    let output = child.wait_with_output().map_err(|e| {
        format!(
            "[{suite_name}] \"{}\": failed to wait for clank: {e}",
            case.name
        )
    })?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let actual_exit = output.status.code().unwrap_or(-1);
    let expected_exit = case.expect_exit as i32;

    let mut errors: Vec<String> = Vec::new();

    // --- exit code ---
    if actual_exit != expected_exit {
        errors.push(format!(
            "  exit code: got {actual_exit}, expected {expected_exit}"
        ));
    }

    // --- exact stdout ---
    if let Some(expected) = &case.expect_stdout {
        if stdout.as_ref() != expected.as_str() {
            errors.push(format!(
                "  stdout mismatch:\n    expected: {expected:?}\n    got:      {stdout:?}"
            ));
        }
    }

    // --- stdout contains ---
    if let Some(needle) = &case.expect_stdout_contains {
        if !stdout.contains(needle.as_str()) {
            errors.push(format!(
                "  stdout does not contain {needle:?}\n    got: {stdout:?}"
            ));
        }
    }

    // --- stdout contains all ---
    for needle in &case.expect_stdout_contains_all {
        if !stdout.contains(needle.as_str()) {
            errors.push(format!(
                "  stdout does not contain {needle:?}\n    got: {stdout:?}"
            ));
        }
    }

    // --- stdout line matches ---
    for pattern in &case.expect_stdout_line_matches {
        let matched = stdout.lines().any(|line| glob_match(pattern, line));
        if !matched {
            errors.push(format!(
                "  no stdout line matches pattern {pattern:?}\n    got: {stdout:?}"
            ));
        }
    }

    // --- stderr empty ---
    if case.expect_stderr_empty && !output.stderr.is_empty() {
        errors.push(format!("  stderr expected empty, got: {stderr:?}"));
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "FAILED [{suite_name}] \"{}\"\n{}",
            case.name,
            errors.join("\n")
        ))
    }
}

// ---------------------------------------------------------------------------
// Harness registration
// ---------------------------------------------------------------------------

// Unit tests for glob_match — registered directly with the datatest harness.
// These functions are prefixed so they're visually distinct from suite runners.

#[test]
fn unit_glob_match_no_wildcard() {
    assert!(glob_match("hello", "hello"));
    assert!(!glob_match("hello", "world"));
}

#[test]
fn unit_glob_match_leading_wildcard() {
    assert!(glob_match(
        "*] command: ls",
        "[2026-03-07T12:00:00Z] command: ls"
    ));
    assert!(!glob_match(
        "*] command: ls",
        "[2026-03-07T12:00:00Z] command: echo"
    ));
}

#[test]
fn unit_glob_match_brackets_around_timestamp() {
    assert!(glob_match(
        "[*] command: echo hello",
        "[2026-03-07T14:00:00Z] command: echo hello"
    ));
    assert!(!glob_match(
        "[*] command: echo hello",
        "2026-03-07T14:00:00Z command: echo hello"
    ));
}

#[test]
fn unit_glob_match_trailing_wildcard() {
    assert!(glob_match("command: echo*", "command: echo hello world"));
}

#[test]
fn unit_glob_match_empty_pattern_matches_empty() {
    assert!(glob_match("", ""));
}

#[test]
fn unit_glob_match_star_only_matches_anything() {
    assert!(glob_match("*", "anything at all"));
    assert!(glob_match("*", ""));
}

// ---------------------------------------------------------------------------
// Harness registration
// ---------------------------------------------------------------------------

datatest_stable::harness! {
    {
        test    = run_suite,
        root    = "cases",
        pattern = r".*\.yaml$",
    },
}
