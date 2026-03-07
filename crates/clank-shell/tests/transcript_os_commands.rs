/// Level 2 integration tests verifying that OS-fallthrough command output
/// is captured into the session transcript.
///
/// These tests cover the core property of the shell: the model sees exactly
/// what the human sees, including output from commands that are resolved via
/// the OS (full-path invocations, $PATH-resolved commands not in the registry).
use std::sync::{Arc, RwLock};

use clank_http::MockHttpClient;
use clank_shell::{ClankShell, EntryKind, Transcript};

async fn make_shell() -> (ClankShell, Arc<RwLock<Transcript>>) {
    let transcript = Arc::new(RwLock::new(Transcript::default()));
    let http = Arc::new(MockHttpClient::new(vec![]));
    let shell = ClankShell::with_http(Arc::clone(&transcript), http)
        .await
        .expect("failed to create shell");
    (shell, transcript)
}

fn output_entries(transcript: &Arc<RwLock<Transcript>>) -> Vec<String> {
    transcript
        .read()
        .unwrap()
        .entries()
        .iter()
        .filter(|e| e.kind == EntryKind::Output)
        .map(|e| e.text.clone())
        .collect()
}

/// /bin/echo is a real OS binary — not in the clank dispatch table.
/// Its output must be captured and appended to the transcript.
#[tokio::test]
async fn test_os_fullpath_output_captured_in_transcript() {
    let (mut shell, transcript) = make_shell().await;
    let code = shell.run_line("/bin/echo hello_from_os").await;
    assert_eq!(code, 0);

    let outputs = output_entries(&transcript);
    assert!(
        outputs.iter().any(|o| o.contains("hello_from_os")),
        "OS command output must appear in transcript; output entries: {outputs:?}"
    );
}

/// An unqualified command resolved via $PATH (not in the registry).
/// `echo` is a bash builtin AND a $PATH binary — in clank it is not registered
/// as a clank builtin, so it falls through to Brush/OS handling.
#[tokio::test]
async fn test_path_resolved_command_output_captured_in_transcript() {
    let (mut shell, transcript) = make_shell().await;
    let code = shell.run_line("echo path_resolved_output").await;
    assert_eq!(code, 0);

    let outputs = output_entries(&transcript);
    assert!(
        outputs.iter().any(|o| o.contains("path_resolved_output")),
        "PATH-resolved command output must appear in transcript; output entries: {outputs:?}"
    );
}

/// An OS pipeline (both commands are real OS binaries).
/// The output of the full pipeline must be captured, not just the first stage.
#[tokio::test]
async fn test_os_pipeline_output_captured_in_transcript() {
    let (mut shell, transcript) = make_shell().await;
    // printf is a reliable built-in on all platforms; grep -c counts matches.
    let code = shell
        .run_line("printf 'alpha\\nbeta\\ngamma\\n' | /usr/bin/grep beta")
        .await;
    assert_eq!(code, 0);

    let outputs = output_entries(&transcript);
    assert!(
        outputs.iter().any(|o| o.contains("beta")),
        "OS pipeline output must appear in transcript; output entries: {outputs:?}"
    );
    // The non-matching lines must NOT appear.
    assert!(
        !outputs
            .iter()
            .any(|o| o.contains("alpha") || o.contains("gamma")),
        "grep must filter; alpha/gamma must not appear: {outputs:?}"
    );
}

/// After the OS command fix, registered shell-internal commands must still
/// work correctly — their output must still appear on stdout and in the
/// transcript.
#[tokio::test]
async fn test_registered_command_still_works_after_os_capture_change() {
    let (mut shell, transcript) = make_shell().await;
    // `ls` is a registered clank command — exercises the registered path.
    let code = shell.run_line("ls /tmp").await;
    assert_eq!(code, 0);
    // ls of /tmp will have output; just verify it wasn't lost.
    let outputs = output_entries(&transcript);
    assert!(
        !outputs.is_empty(),
        "registered command (ls) must still produce transcript output"
    );
}

/// Output from consecutive OS commands must not bleed across invocations.
/// The first command's output must not appear in the second command's entry.
#[tokio::test]
async fn test_capture_does_not_bleed_across_commands() {
    let (mut shell, transcript) = make_shell().await;
    shell.run_line("/bin/echo first_output").await;
    shell.run_line("/bin/echo second_output").await;

    let outputs = output_entries(&transcript);
    assert_eq!(
        outputs.len(),
        2,
        "must be exactly 2 output entries; got: {outputs:?}"
    );
    assert!(
        outputs[0].contains("first_output"),
        "first entry must contain first_output: {:?}",
        outputs[0]
    );
    assert!(
        outputs[1].contains("second_output"),
        "second entry must contain second_output: {:?}",
        outputs[1]
    );
    assert!(
        !outputs[0].contains("second_output"),
        "first entry must not bleed second_output: {:?}",
        outputs[0]
    );
    assert!(
        !outputs[1].contains("first_output"),
        "second entry must not bleed first_output: {:?}",
        outputs[1]
    );
}

/// A command with no output must produce no transcript output entry.
/// This verifies the output.is_empty() guard in run_line.
#[tokio::test]
async fn test_silent_command_produces_no_output_entry() {
    let (mut shell, transcript) = make_shell().await;
    let code = shell.run_line("true").await;
    assert_eq!(code, 0);

    let outputs = output_entries(&transcript);
    assert!(
        outputs.is_empty(),
        "silent command must produce no output entry; got: {outputs:?}"
    );
}
