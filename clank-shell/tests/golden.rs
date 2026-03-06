//! Golden test matrix for the `clank` binary.
//!
//! Each subdirectory of `tests/golden/` is a test case:
//!
//!   tests/golden/<case>/
//!     stdin       — input fed verbatim to clank's stdin (required; hand-authored)
//!     stdout      — expected stdout (golden file; auto-updatable)
//!     exit_code   — expected exit code as a plain integer string (golden file; auto-updatable)
//!     transcript  — expected transcript as JSON (golden file; auto-updatable)
//!
//! To update golden files after an intentional behaviour change:
//!
//!   UPDATE_GOLDENFILES=1 cargo test --test golden
//!
//! Then review `git diff tests/golden/` and commit the changes.
//!
//! To add a new case, create a new directory with a `stdin` file and run the
//! update command above — `stdout`, `exit_code`, and `transcript` will be
//! populated automatically.

use goldenfile::Mint;
use std::io::Write as _;
use std::path::Path;
use std::process::Command;
use test_r::test_gen;

test_r::enable!();

/// Locate the compiled `clank` binary via the env var set by cargo.
fn clank_bin() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_BIN_EXE_clank"))
}

/// Directory containing all golden test cases, relative to the workspace root.
/// `CARGO_MANIFEST_DIR` points to `clank-shell/`.
fn golden_dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("golden")
}

/// Run a single golden test case.
///
/// 1. Read `stdin` from the case directory.
/// 2. Invoke the `clank` binary with that stdin, passing `--dump-transcript`
///    to write the session transcript to a temp file.
/// 3. Write actual stdout, exit code, and transcript to a `goldenfile::Mint`.
///    The mint diffs against the checked-in golden files on drop.
fn run_case(case_dir: &Path) {
    let stdin_path = case_dir.join("stdin");
    assert!(
        stdin_path.exists(),
        "Golden case {:?} is missing a `stdin` file",
        case_dir
    );

    let stdin_content = std::fs::read(&stdin_path)
        .unwrap_or_else(|e| panic!("Failed to read {:?}: {e}", stdin_path));

    // Temp file for the transcript dump.
    let transcript_tmp =
        tempfile::NamedTempFile::new().expect("Failed to create temp file for transcript");

    // Run the binary.
    let output = Command::new(clank_bin())
        .arg("--dump-transcript")
        .arg(transcript_tmp.path())
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null()) // stderr is diagnostic; not part of the contract
        .spawn()
        .and_then(|mut child| {
            child.stdin.take().unwrap().write_all(&stdin_content).ok();
            child.wait_with_output()
        })
        .unwrap_or_else(|e| panic!("Failed to run clank binary: {e}"));

    let actual_stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let actual_exit_code = output.status.code().unwrap_or(-1).to_string();
    let actual_transcript =
        std::fs::read_to_string(transcript_tmp.path()).unwrap_or_else(|_| "[]".to_string());

    // Compare (or update) golden files via goldenfile::Mint.
    let mut mint = Mint::new(case_dir);

    let mut stdout_file = mint
        .new_goldenfile("stdout")
        .unwrap_or_else(|e| panic!("Failed to open golden stdout for {:?}: {e}", case_dir));
    write!(stdout_file, "{actual_stdout}").unwrap();

    let mut exit_code_file = mint
        .new_goldenfile("exit_code")
        .unwrap_or_else(|e| panic!("Failed to open golden exit_code for {:?}: {e}", case_dir));
    writeln!(exit_code_file, "{actual_exit_code}").unwrap();

    let mut transcript_file = mint
        .new_goldenfile("transcript")
        .unwrap_or_else(|e| panic!("Failed to open golden transcript for {:?}: {e}", case_dir));
    write!(transcript_file, "{actual_transcript}").unwrap();

    // Mint::drop performs the diff. If UPDATE_GOLDENFILES=1 is set it writes
    // instead. Either way, nothing more to do here.
}

/// Discover all test cases and register each as a named test with test-r.
#[test_gen]
fn golden(tests: &mut test_r::core::DynamicTestRegistration) {
    let dir = golden_dir();
    if !dir.exists() {
        return;
    }

    let mut entries: Vec<_> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("Cannot read golden dir {dir:?}: {e}"))
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .collect();

    // Sort for deterministic ordering.
    entries.sort_by_key(|e| e.file_name());

    for entry in entries {
        let case_dir = entry.path();
        let case_name = case_dir
            .file_name()
            .unwrap()
            .to_string_lossy()
            .replace('-', "_");

        tests.add_sync_test(
            &case_name,
            test_r::core::TestProperties::default(),
            None,
            move |_deps| run_case(&case_dir),
        );
    }
}
