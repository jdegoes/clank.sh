/// Authorization enforcement tests (Level 2).
///
/// Tests are organised into two sides:
///
/// **Side A — User context (`run_line`):** Authorization policies are NOT
/// enforced. The user has already authorised the command by typing it. This
/// is the correct Phase 1 behaviour for all interactive shell use.
///
/// **Side B — Agent context (`run_line_as_agent`):** `Confirm` and `SudoOnly`
/// policies ARE enforced. This is the Phase 3 behaviour for commands issued
/// autonomously by an AI agent. `run_line_as_agent` is not called from any
/// production code in Phase 1; it exists as the Phase 3 entry point and as
/// the test surface for agent-context enforcement.
///
/// See: dev-docs/issues/open/authorization-context-user-vs-agent.md
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

// ---------------------------------------------------------------------------
// Side A — User context: policies are NOT enforced
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_user_confirm_command_executes_without_prompt() {
    // `mkdir` has a `Confirm` policy intended for agent context. When a human
    // types it, it must execute immediately without any confirmation prompt.
    // We verify this by checking that the directory is actually created (exit 0)
    // rather than aborted (exit 1).
    let dir = tempfile::tempdir().unwrap();
    let new_dir = dir.path().join("clank-auth-user-mkdir-test");
    let mut shell = make_shell().await;

    let code = shell
        .run_line(&format!("mkdir {}", new_dir.display()))
        .await;

    assert_eq!(
        code, 0,
        "mkdir must succeed without confirmation in user context"
    );
    assert!(
        new_dir.is_dir(),
        "directory must be created — prompt must not have blocked execution"
    );
}

#[tokio::test]
async fn test_user_sudo_only_command_executes_without_sudo() {
    // `rm` has a `SudoOnly` policy intended for agent context. When a human
    // types it without `sudo`, it must execute. The file doesn't exist so we
    // get exit 1 (rm's own error), not exit 5 (authorization denied).
    let mut shell = make_shell().await;
    let code = shell
        .run_line("rm /tmp/clank-auth-user-rm-nonexistent-test")
        .await;

    assert_ne!(
        code, 5,
        "rm without sudo must not be denied (exit 5) in user context"
    );
    assert_eq!(
        code, 1,
        "rm on a nonexistent path must exit 1 (rm's own error), not 5"
    );
}

#[tokio::test]
async fn test_user_sudo_prefix_still_strips_and_executes() {
    // `sudo rm` must still work in user context — the prefix is stripped and
    // the underlying command executes normally. This verifies the sudo-stripping
    // mechanism is not broken by the authorization bypass.
    let mut shell = make_shell().await;
    let code = shell
        .run_line("sudo rm /tmp/clank-auth-user-sudo-rm-nonexistent-test")
        .await;

    assert_ne!(code, 5, "sudo rm must not be denied in user context");
    assert_eq!(
        code, 1,
        "sudo rm on a nonexistent path must exit 1 (rm's own error)"
    );
}

// ---------------------------------------------------------------------------
// Side B — Agent context: policies ARE enforced
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_agent_sudo_only_denied_without_sudo() {
    // In agent context, `rm` without `sudo` must be denied (exit 5).
    // This is the core SudoOnly safety gate for Phase 3.
    let mut shell = make_shell().await;
    let code = shell
        .run_line_as_agent("rm /tmp/clank-auth-agent-rm-nonexistent-test")
        .await;

    assert_eq!(
        code, 5,
        "SudoOnly command without sudo must exit 5 in agent context"
    );
}

#[tokio::test]
async fn test_agent_sudo_prefix_is_denied() {
    // Per spec: "Agents cannot use sudo. An agent that needs elevation must
    // pause and surface a confirmation request." (README.md § Authorization)
    //
    // A `sudo` prefix in agent context must be denied immediately (exit 5).
    // Elevation for an agent comes only from the human having invoked
    // `sudo ask`, which is a separate mechanism not handled at this layer.
    let mut shell = make_shell().await;
    let code = shell
        .run_line_as_agent("sudo rm /tmp/clank-auth-agent-sudo-test")
        .await;

    assert_eq!(
        code, 5,
        "sudo prefix must be denied (exit 5) in agent context — agents cannot use sudo"
    );
}

#[tokio::test]
async fn test_agent_confirm_command_aborts_without_grant() {
    // In agent context, `mkdir` has a `Confirm` policy. With stdin wired to
    // /dev/null, read_line returns EOF → empty answer → denial → exit 1.
    // The directory must not be created.
    let dir = tempfile::tempdir().unwrap();
    let new_dir = dir.path().join("clank-auth-agent-mkdir-test");
    let mut shell = make_shell().await;

    // Redirect stdin from /dev/null so the confirmation prompt receives EOF.
    let code = shell
        .run_line_as_agent(&format!("mkdir {} </dev/null", new_dir.display()))
        .await;

    assert_eq!(
        code, 1,
        "Confirm command with no grant must abort (exit 1) in agent context"
    );
    assert!(
        !new_dir.exists(),
        "directory must not be created when confirmation is denied"
    );
}
