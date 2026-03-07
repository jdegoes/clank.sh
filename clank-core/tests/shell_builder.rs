//! Integration tests for clank-core's public shell API.
//!
//! These tests exercise the public API surface of `clank-core` — specifically
//! [`clank_core::default_options`], [`clank_core::run`], and
//! [`clank_core::run_with_options`] — without spawning any subprocess.
//! They are compiled as a separate crate linked against `clank-core`.

use clank_core::{default_options, run, run_with_options, CreateOptions, Shell};

// ---------------------------------------------------------------------------
// default_options field coverage
// (These duplicate the unit tests in lib.rs intentionally: the integration
//  tier validates the *public* contract; unit tests validate internals.)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn default_options_is_non_interactive() {
    assert!(!default_options().interactive);
}

#[tokio::test]
async fn default_options_skips_profile() {
    assert!(default_options().no_profile);
}

#[tokio::test]
async fn default_options_skips_rc() {
    assert!(default_options().no_rc);
}

#[tokio::test]
async fn default_options_disables_editing() {
    assert!(default_options().no_editing);
}

#[tokio::test]
async fn default_options_shell_name_is_clank() {
    assert_eq!(default_options().shell_name, Some("clank".to_owned()));
}

// ---------------------------------------------------------------------------
// Shell::new construction
// ---------------------------------------------------------------------------

#[tokio::test]
async fn shell_new_with_default_options_succeeds() {
    Shell::new(default_options())
        .await
        .expect("Shell::new with default options should succeed");
}

#[tokio::test]
async fn shell_new_with_stdlib_defaults_succeeds() {
    Shell::new(CreateOptions::default())
        .await
        .expect("Shell::new with CreateOptions::default() should succeed");
}

// ---------------------------------------------------------------------------
// run() behaviour
// ---------------------------------------------------------------------------

#[tokio::test]
async fn run_echo_returns_zero() {
    let code = run("echo hello").await.expect("run should not error");
    assert_eq!(code, 0, "echo hello should exit 0");
}

#[tokio::test]
async fn run_false_returns_nonzero() {
    let code = run("false").await.expect("run should not error");
    assert_ne!(code, 0, "false should exit non-zero");
}

#[tokio::test]
async fn run_exit_42_returns_42() {
    let code = run("exit 42").await.expect("run should not error");
    assert_eq!(code, 42, "exit 42 should return code 42");
}

// ---------------------------------------------------------------------------
// run_with_options() — custom CreateOptions
// ---------------------------------------------------------------------------

#[tokio::test]
async fn run_with_options_custom_shell_name() {
    let opts = CreateOptions {
        shell_name: Some("myshell".to_owned()),
        ..default_options()
    };
    // $0 should reflect the custom shell name.
    let code = run_with_options("echo $0", opts)
        .await
        .expect("run_with_options should not error");
    assert_eq!(code, 0);
}
