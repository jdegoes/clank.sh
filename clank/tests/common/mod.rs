use assert_cmd::assert::Assert;
use assert_cmd::Command;

/// Returns a Command configured to run the clank binary.
pub fn clank() -> Command {
    Command::new(assert_cmd::cargo::cargo_bin!("clank"))
}

/// Runs a multi-line script through clank's stdin and returns the Assert handle.
/// Each line in `script` is sent as a separate stdin line.
/// A trailing newline is automatically added if not present.
pub fn run_script(script: &str) -> Assert {
    let input = if script.ends_with('\n') {
        script.to_string()
    } else {
        format!("{script}\n")
    };
    clank().write_stdin(input).assert()
}
