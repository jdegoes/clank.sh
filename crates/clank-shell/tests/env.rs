/// Level 2 integration tests for the `env` command via production dispatch.
///
/// These tests cover the full dispatch path (builtins.rs → EnvProcess) to
/// verify that `ctx.env` is populated from the real environment, not left empty.
use std::sync::{Arc, RwLock};

use clank_http::MockHttpClient;
use clank_shell::{secrets::SecretsRegistry, ClankShell, EntryKind, Transcript};

async fn make_shell() -> (ClankShell, Arc<RwLock<Transcript>>) {
    let transcript = Arc::new(RwLock::new(Transcript::default()));
    let http = Arc::new(MockHttpClient::new(vec![]));
    let shell = ClankShell::with_http(Arc::clone(&transcript), http)
        .await
        .expect("failed to create shell");
    (shell, transcript)
}

#[tokio::test]
async fn test_env_command_shows_exported_variable() {
    // Set a unique env var and verify env prints it.
    std::env::set_var("CLANK_ENV_TEST_VAR", "hello_from_env");
    let (mut shell, transcript) = make_shell().await;
    let code = shell.run_line("env").await;
    std::env::remove_var("CLANK_ENV_TEST_VAR");

    assert_eq!(code, 0);
    let t = transcript.read().unwrap();
    let output: String = t
        .entries()
        .iter()
        .filter(|e| e.kind == EntryKind::Output)
        .map(|e| e.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        output.contains("CLANK_ENV_TEST_VAR=hello_from_env"),
        "env must show exported variable in transcript output; got:\n{output}"
    );
}

#[tokio::test]
async fn test_env_masks_secret_variable() {
    // Set a secret variable and verify env masks its value.
    std::env::set_var("CLANK_ENV_SECRET_TEST", "should_be_hidden");
    SecretsRegistry::insert("CLANK_ENV_SECRET_TEST");

    let (mut shell, transcript) = make_shell().await;
    let code = shell.run_line("env").await;

    SecretsRegistry::remove("CLANK_ENV_SECRET_TEST");
    std::env::remove_var("CLANK_ENV_SECRET_TEST");

    assert_eq!(code, 0);
    let t = transcript.read().unwrap();
    let output: String = t
        .entries()
        .iter()
        .filter(|e| e.kind == EntryKind::Output)
        .map(|e| e.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        output.contains("CLANK_ENV_SECRET_TEST=***"),
        "env must mask secret variable value; got:\n{output}"
    );
    assert!(
        !output.contains("should_be_hidden"),
        "plaintext secret value must not appear in env output; got:\n{output}"
    );
}
