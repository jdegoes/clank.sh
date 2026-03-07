use std::sync::{Arc, RwLock};

use clank_shell::process_table;
use clank_shell::{ClankShell, EntryKind, Transcript};

/// Test that a registered subprocess command (one that goes through the
/// dual-path temp-file capture mechanism in `run_line`) has its stdout
/// recorded as an `Output` entry in the transcript.
///
/// This is distinct from Brush builtins like `echo`, which run in-process and
/// bypass the capture path entirely. A failure here means the transcript — and
/// therefore the AI model's context window — will be missing command output.
#[tokio::test]
async fn test_registered_command_output_captured_in_transcript() {
    let (mut shell, transcript) = shell_with_transcript().await;

    // `ls /` is a registered clank subprocess command. Its output goes through
    // the temp-file capture path, not through Brush's in-process stdout.
    // `/` is guaranteed to exist and contain at least one well-known entry on
    // any Unix system.
    let code = shell.run_line("ls /").await;
    assert_eq!(code, 0, "ls / must exit 0");

    let t = transcript.read().unwrap();
    let output_entries: Vec<_> = t
        .entries()
        .iter()
        .filter(|e| e.kind == EntryKind::Output)
        .collect();

    assert!(
        !output_entries.is_empty(),
        "ls / must produce at least one Output entry in the transcript"
    );

    // The root directory always contains at least one of these on any Unix system.
    let all_output = output_entries
        .iter()
        .map(|e| e.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let has_known_entry = ["usr", "bin", "etc", "tmp", "var", "home", "lib"]
        .iter()
        .any(|name| all_output.contains(name));
    assert!(
        has_known_entry,
        "ls / output must contain a known root directory entry; got: {all_output:?}"
    );
}

async fn shell_with_transcript() -> (ClankShell, Arc<RwLock<Transcript>>) {
    let transcript = Arc::new(RwLock::new(Transcript::default()));
    let shell = ClankShell::with_transcript(Arc::clone(&transcript))
        .await
        .expect("failed to create shell");
    (shell, transcript)
}

#[tokio::test]
async fn test_output_captured_in_transcript() {
    let (mut shell, transcript) = shell_with_transcript().await;

    shell.run_line("echo hello").await;

    let t = transcript.read().unwrap();
    let output_entries: Vec<_> = t
        .entries()
        .iter()
        .filter(|e| e.kind == EntryKind::Output)
        .collect();

    assert!(
        !output_entries.is_empty(),
        "expected at least one Output entry, got none"
    );
    assert!(
        output_entries.iter().any(|e| e.text.contains("hello")),
        "expected 'hello' in output entries, got: {:?}",
        output_entries.iter().map(|e| &e.text).collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn test_command_captured_in_transcript() {
    let (mut shell, transcript) = shell_with_transcript().await;

    shell.run_line("echo world").await;

    let t = transcript.read().unwrap();
    let cmd_entries: Vec<_> = t
        .entries()
        .iter()
        .filter(|e| e.kind == EntryKind::Command)
        .collect();

    assert!(
        cmd_entries.iter().any(|e| e.text.contains("echo world")),
        "command not found in transcript"
    );
}

#[tokio::test]
async fn test_output_displayed_to_user() {
    // Smoke test: run_line completes without hanging, returns exit code 0.
    let (mut shell, _) = shell_with_transcript().await;
    let code = shell.run_line("echo hello").await;
    assert_eq!(code, 0);
}

#[tokio::test]
async fn test_multiple_commands_each_captured() {
    let (mut shell, transcript) = shell_with_transcript().await;

    shell.run_line("echo first").await;
    shell.run_line("echo second").await;

    let t = transcript.read().unwrap();
    let texts: Vec<_> = t.entries().iter().map(|e| e.text.as_str()).collect();
    assert!(
        texts.iter().any(|t| t.contains("first")),
        "first output missing"
    );
    assert!(
        texts.iter().any(|t| t.contains("second")),
        "second output missing"
    );
}

/// D7: `/proc/<pid>/environ` is populated from `std::env::vars()`, not empty.
///
/// Strategy: spawn a sentinel process in the process table before calling
/// `run_line`, so the snapshot taken at the start of `run_line` includes it.
/// Then `cat /proc/<pid>/environ` should return non-empty content containing
/// at least one well-known env var (HOME or PATH).
#[tokio::test]
async fn test_proc_environ_not_empty() {
    let (mut shell, transcript) = shell_with_transcript().await;

    // Spawn a sentinel entry so the snapshot has a known PID.
    let shell_id = shell.shell_id();
    let pid = process_table::spawn(
        shell_id,
        0,
        vec!["test-sentinel".to_string()],
        clank_shell::ProcessType::Subprocess,
    );

    // run_line refreshes proc_snapshot at start — the sentinel will be included.
    let path = format!("/proc/{pid}/environ");
    let code = shell.run_line(&format!("cat {path}")).await;
    assert_eq!(code, 0, "cat /proc/<pid>/environ should exit 0");

    // Reap sentinel so subsequent test isolation is maintained.
    process_table::reap(shell_id, pid);

    // The output should contain at least PATH or HOME.
    let t = transcript.read().unwrap();
    let all_output: String = t
        .entries()
        .iter()
        .filter(|e| e.kind == EntryKind::Output)
        .map(|e| e.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");

    let has_env = all_output.contains("PATH=") || all_output.contains("HOME=");
    assert!(
        has_env,
        "expected PATH or HOME in /proc/<pid>/environ output; got: {all_output:?}"
    );
}
