/// Working directory synchronisation tests.
///
/// These tests verify that VFS commands resolve relative paths against
/// Brush's internal working directory — not against `std::env::current_dir()`
/// (the OS process cwd, which is never updated when `cd` runs).
///
/// Every test in this file follows `cd` with a VFS command. This is the
/// combination that was not tested before and that caused the bug where
/// `mkdir demo && cd demo` would fail with "No such file or directory".
use std::sync::{Arc, RwLock};

use clank_http::MockHttpClient;
use clank_shell::{ClankShell, Transcript};

async fn make_shell() -> ClankShell {
    let transcript = Arc::new(RwLock::new(Transcript::default()));
    let http = Arc::new(MockHttpClient::new(vec![]));
    ClankShell::with_http(transcript, http)
        .await
        .expect("failed to create shell")
}

/// W5a — The exact scenario the user hit.
///
/// `mkdir` after `cd` must create the directory relative to the new working
/// directory, not the OS process cwd at shell launch time. Before the fix,
/// `mkdir demo` after `cd /tmp` would create `<launch_dir>/demo` instead of
/// `/tmp/demo`, and the subsequent `cd demo` would fail.
#[tokio::test]
async fn test_mkdir_after_cd_creates_in_new_cwd() {
    let mut shell = make_shell().await;

    let dir_name = format!("clank-wd-test-{}", std::process::id());
    let expected = std::path::PathBuf::from("/tmp").join(&dir_name);

    // Ensure clean state.
    let _ = std::fs::remove_dir_all(&expected);

    let cd_code = shell.run_line("cd /tmp").await;
    assert_eq!(cd_code, 0, "cd /tmp must succeed");

    let mkdir_code = shell.run_line(&format!("mkdir {dir_name}")).await;
    assert_eq!(mkdir_code, 0, "mkdir must succeed after cd");

    assert!(
        expected.is_dir(),
        "directory must be created in /tmp, not in the launch directory: {expected:?}"
    );

    // Clean up.
    let _ = std::fs::remove_dir_all(&expected);
}

/// W5b — Read path: `ls` after `cd` shows contents of the new directory.
///
/// Verifies that the read-path VFS commands resolve relative paths correctly
/// after `cd`. Before the fix, `ls` after `cd /tmp` would list `<launch_dir>`
/// instead of `/tmp`.
#[tokio::test]
async fn test_ls_after_cd_shows_new_directory_contents() {
    let dir = tempfile::tempdir().unwrap();
    let sentinel = dir.path().join("clank-sentinel-file.txt");
    std::fs::write(&sentinel, "sentinel").unwrap();

    let mut shell = make_shell().await;
    let dir_path = dir.path().to_str().unwrap().to_string();

    let cd_code = shell.run_line(&format!("cd {dir_path}")).await;
    assert_eq!(cd_code, 0, "cd must succeed");

    // Capture ls output via transcript.
    let transcript = Arc::new(RwLock::new(Transcript::default()));
    let http = Arc::new(MockHttpClient::new(vec![]));
    let mut shell2 = ClankShell::with_http(Arc::clone(&transcript), http)
        .await
        .expect("shell");

    let cd_code = shell2.run_line(&format!("cd {dir_path}")).await;
    assert_eq!(cd_code, 0, "cd must succeed");

    let ls_code = shell2.run_line("ls").await;
    assert_eq!(ls_code, 0, "ls must succeed after cd");

    let t = transcript.read().unwrap();
    let all_output: String = t
        .entries()
        .iter()
        .filter(|e| e.kind == clank_shell::EntryKind::Output)
        .map(|e| e.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        all_output.contains("clank-sentinel-file.txt"),
        "ls must list the sentinel file in the new cwd; got: {all_output:?}"
    );
}

/// W5c — Chained `cd` with relative `mkdir`: cwd is cumulative.
///
/// Verifies that path resolution is correct across multiple `cd` invocations.
/// Each `cd` must update the cwd for all subsequent VFS commands, not just the
/// first one.
#[tokio::test]
async fn test_chained_cd_relative_mkdir() {
    let mut shell = make_shell().await;

    let outer = format!("clank-wd-chain-outer-{}", std::process::id());
    let outer_path = std::path::PathBuf::from("/tmp").join(&outer);
    let inner_path = outer_path.join("inner");

    // Clean up any residue from a previous run.
    let _ = std::fs::remove_dir_all(&outer_path);
    std::fs::create_dir_all(&outer_path).expect("test setup: create outer dir");

    // cd /tmp, mkdir <outer> already created above, cd into it, mkdir inner.
    let code = shell.run_line("cd /tmp").await;
    assert_eq!(code, 0, "cd /tmp");

    let code = shell.run_line(&format!("cd {outer}")).await;
    assert_eq!(code, 0, "cd into outer dir");

    let code = shell.run_line("mkdir inner").await;
    assert_eq!(code, 0, "mkdir inner after chained cd");

    assert!(
        inner_path.is_dir(),
        "inner dir must exist at {inner_path:?} after chained cd + mkdir"
    );

    // Clean up.
    let _ = std::fs::remove_dir_all(&outer_path);
}
