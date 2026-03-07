use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;

fn clank() -> Command {
    assert_cmd::cargo_bin_cmd!("clank")
}

// ---------------------------------------------------------------------------
// Shell behaviour tests (acceptance tests 1–12)
// ---------------------------------------------------------------------------

#[test]
fn test_echo() {
    clank()
        .write_stdin("echo \"hello world\"\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("hello world"));
}

#[test]
fn test_export_and_expand() {
    clank()
        .write_stdin("export FOO=bar && echo $FOO\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("bar"));
}

#[test]
fn test_pwd() {
    let cwd = std::env::current_dir()
        .unwrap()
        .to_string_lossy()
        .to_string();
    clank()
        .write_stdin("pwd\n")
        .assert()
        .success()
        .stdout(predicate::str::contains(cwd));
}

#[test]
fn test_cd_and_pwd() {
    clank()
        .write_stdin("cd /tmp && pwd\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("/tmp"));
}

#[test]
fn test_pipe() {
    // Use only echo (a Brush builtin) on both sides so no stub is invoked.
    clank()
        .write_stdin("echo \"hello\" | echo \"piped\"\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("piped"));
}

#[test]
fn test_stdout_redirect() {
    let tmp = std::env::temp_dir().join("clank-test-redirect.txt");
    let path = tmp.to_string_lossy().to_string();
    let _ = fs::remove_file(&tmp);

    // Write a file with echo redirect; read it back with the `<` redirection
    // into echo via command substitution — stays within Brush builtins only.
    clank()
        .write_stdin(format!("echo \"hello\" > {path} && echo $(< {path})\n"))
        .assert()
        .success()
        .stdout(predicate::str::contains("hello"));

    let _ = fs::remove_file(&tmp);
}

#[test]
fn test_and_short_circuits_on_failure() {
    clank()
        .write_stdin("false && echo \"should not print\"\n")
        .assert()
        .stdout(predicate::str::contains("should not print").not());
}

#[test]
fn test_or_runs_on_failure() {
    clank()
        .write_stdin("false || echo \"ran\"\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("ran"));
}

#[test]
fn test_semicolon_sequences_regardless_of_exit() {
    clank()
        .write_stdin("false ; echo \"ran\"\n")
        .assert()
        .stdout(predicate::str::contains("ran"));
}

#[test]
fn test_multiline_buffering() {
    // Verify the interactive loop accumulates lines until a multi-line
    // construct is complete before executing. Uses if/fi (Brush builtins only).
    clank()
        .write_stdin("if true; then\necho \"multiline\"\nfi\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("multiline"));
}

#[test]
fn test_heredoc() {
    // Verify heredoc syntax works end-to-end: the shell buffers lines until the
    // terminator is seen, then delivers the body on cat's stdin. cat is now a
    // real implementation so the full pipeline is exercised.
    let output = clank()
        .write_stdin("cat <<EOF\nhello from heredoc\nEOF\n")
        .output()
        .unwrap();

    // Process must not be killed by a signal (no panic).
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        assert!(
            output.status.signal().is_none(),
            "clank was killed by a signal on heredoc (panic?)"
        );
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("hello from heredoc"),
        "expected heredoc content in stdout, got: {stdout}"
    );
}

#[test]
fn test_script_file() {
    let tmp = std::env::temp_dir().join("clank-test-script.sh");
    fs::write(&tmp, "#!/bin/sh\necho \"from script\"\n").unwrap();
    let path = tmp.to_string_lossy().to_string();

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&tmp, fs::Permissions::from_mode(0o755)).unwrap();
    }

    clank()
        .write_stdin(format!("{path}\n"))
        .assert()
        .stdout(predicate::str::contains("from script"));

    let _ = fs::remove_file(&tmp);
}

#[test]
fn test_ls_nonexistent_path() {
    // `ls` is a real implementation; a nonexistent path produces an error on stderr.
    clank()
        .write_stdin("ls /nonexistent-clank-test-path-xyz\n")
        .assert()
        .stderr(predicate::str::is_empty().not());
}

// ---------------------------------------------------------------------------
// Process trait stub behaviour tests (acceptance tests 13–14)
// ---------------------------------------------------------------------------

#[test]
fn test_ask_no_config() {
    // `ask` is now a real command. Without a configured API key it must:
    //   - write an informative error to stderr
    //   - not crash (process exits cleanly)
    //
    // This test runs without any ask.toml config (CI environment).
    // If a real API key is configured on this machine the test is skipped.
    if std::env::var("ANTHROPIC_API_KEY").is_ok() {
        return; // skip — would make a real API call
    }
    let dir = tempfile::tempdir().expect("tempdir");
    clank()
        .env("CLANK_CONFIG", dir.path().join("ask.toml"))
        .write_stdin("ask \"hello\"\n")
        .assert()
        .stderr(
            predicate::str::contains("no API key").or(predicate::str::contains("not configured")),
        );
}

#[test]
fn test_unknown_command_error() {
    // A command not in the dispatch table falls through to Brush's $PATH
    // resolution and produces a "command not found" error on stderr.
    clank()
        .write_stdin("totally-nonexistent-clank-command-xyz\n")
        .assert()
        .stderr(predicate::str::contains("not found").or(predicate::str::contains("not yet")));
}

// ---------------------------------------------------------------------------
// Brush known-gap graceful failure tests (acceptance tests 15–16)
// ---------------------------------------------------------------------------

#[test]
fn test_coproc_no_panic() {
    // `coproc` may or may not be supported. Either way, the process must exit
    // cleanly — not be killed by a signal (which would indicate a panic).
    let output = clank()
        .write_stdin("coproc { echo hi; }\n")
        .output()
        .unwrap();

    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        assert!(
            output.status.signal().is_none(),
            "clank was killed by a signal on coproc (panic?): {:?}",
            output.status.signal()
        );
    }
}

#[test]
fn test_select_no_panic() {
    // `select` is not fully supported by Brush. Must fail gracefully, not panic.
    let output = clank()
        .write_stdin("select x in a b c; do echo $x; done\n")
        .output()
        .unwrap();

    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        assert!(
            output.status.signal().is_none(),
            "clank was killed by a signal on select (panic?): {:?}",
            output.status.signal()
        );
    }
}
