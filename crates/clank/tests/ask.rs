use assert_cmd::cargo_bin_cmd;
use predicates::prelude::*;

/// Return an isolated config path in a temp directory, and set CLANK_CONFIG
/// so the binary reads/writes there rather than the real user config.
///
/// Returns the `tempfile::TempDir` — the caller must keep it alive for the
/// duration of the test.
fn isolated_clank() -> (assert_cmd::Command, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("tempdir");
    let config_path = dir.path().join("ask.toml");
    let mut cmd = cargo_bin_cmd!("clank");
    cmd.env("CLANK_CONFIG", &config_path);
    (cmd, dir)
}

/// Return true if a real API key is configured at either the CLANK_CONFIG
/// override path or the platform default path.
fn has_api_key() -> bool {
    // Check CLANK_CONFIG override first (consistent with what the binary reads).
    let path = if let Ok(v) = std::env::var("CLANK_CONFIG") {
        if !v.is_empty() {
            std::path::PathBuf::from(v)
        } else {
            dirs_next::config_dir()
                .unwrap_or_default()
                .join("ask")
                .join("ask.toml")
        }
    } else {
        dirs_next::config_dir()
            .unwrap_or_default()
            .join("ask")
            .join("ask.toml")
    };

    if let Ok(contents) = std::fs::read_to_string(&path) {
        return contents.contains("api_key");
    }
    false
}

#[test]
fn test_ask_no_config_exits_with_message() {
    if has_api_key() {
        return; // skip on machines with real config
    }
    let (mut cmd, _dir) = isolated_clank();
    cmd.write_stdin("ask \"hello\"\n").assert().stderr(
        predicate::str::contains("no API key").or(predicate::str::contains("not configured")),
    );
}

#[test]
fn test_ask_bad_args_exits_stderr() {
    let (mut cmd, _dir) = isolated_clank();
    cmd.write_stdin("ask --unknown-flag-xyz\n")
        .assert()
        .stderr(predicate::str::contains("unknown flag").or(predicate::str::contains("usage")));
}

#[test]
fn test_context_show_empty() {
    // When the transcript is empty, context show exits 0. The "(transcript is
    // empty)" message is written to the process's stdout handle which at the
    // binary level routes through the shell's real stdout — so it appears in
    // the binary's stdout. However, because context is a shell-internal command
    // the output is not captured into the temp file used for transcript recording;
    // it goes directly to the binary's stdout and is visible in the test output.
    let (mut cmd, _dir) = isolated_clank();
    cmd.write_stdin("context show\n").assert().success();
}

#[test]
fn test_context_clear_succeeds() {
    let (mut cmd, _dir) = isolated_clank();
    cmd.write_stdin("context clear\n")
        .assert()
        .success()
        .stderr(predicate::str::is_empty());
}

#[test]
fn test_context_show_after_command() {
    // After running a command, context show should include it.
    let (mut cmd, _dir) = isolated_clank();
    cmd.write_stdin("echo hi\ncontext show\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("echo hi"));
}

#[test]
fn test_model_list_no_config() {
    // With an isolated empty config directory, model list must report no providers.
    let (mut cmd, _dir) = isolated_clank();
    cmd.write_stdin("model list\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("No providers configured."));
}

#[test]
fn test_model_no_subcommand() {
    let (mut cmd, _dir) = isolated_clank();
    // model errors go to stderr; stdout should only contain the shell prompt.
    cmd.write_stdin("model\n")
        .assert()
        .stderr(predicate::str::contains("usage"));
}
