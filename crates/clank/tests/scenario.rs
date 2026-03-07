/// Scenario test harness for the clank binary.
///
/// Each test case is a single YAML file under `tests/scenarios/`. The file
/// captures input state (env vars, config, files), a stdin session, expected
/// stdout/stderr, and expected resulting state (config file, files on disk).
///
/// ## Running
///
/// ```sh
/// cargo test --test scenario
/// ```
///
/// ## Regenerating expected output
///
/// When `CLANK_UPDATE=1` is set, the harness runs the binary and writes the
/// actual stdout/stderr back into the fixture file instead of asserting.
/// Use this when output changes intentionally.
///
/// ```sh
/// CLANK_UPDATE=1 cargo test --test scenario
/// ```
///
/// ## Filtering
///
/// ```sh
/// cargo test --test scenario -- scenario_tests echo
/// ```
///
/// This runs only scenarios whose file path contains "echo".
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use assert_cmd::cargo_bin_cmd;
use serde::Deserialize;
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Scenario data model
// ---------------------------------------------------------------------------

/// A single test scenario, deserialised from a `.yaml` fixture file.
#[derive(Debug, Deserialize)]
struct Scenario {
    /// Human-readable description. Present in YAML for human readers; not
    /// used programmatically (the file path serves as the test identifier).
    #[serde(default)]
    _desc: Option<String>,

    /// Environment variables injected into the clank process.
    ///
    /// The token `{config}` expands to the isolated config file path.
    /// The token `{cwd}` expands to the sandbox directory path.
    ///
    /// `CLANK_CONFIG` is always set to the isolated config path unless
    /// explicitly overridden here.
    #[serde(default)]
    env: HashMap<String, String>,

    /// Initial config file contents as a TOML value.
    ///
    /// If present, serialised to the isolated config path before the test.
    /// If absent, no config file exists before the test.
    #[serde(default)]
    config: Option<toml::Value>,

    /// Files to pre-populate in the sandbox directory.
    ///
    /// Keys are paths relative to the sandbox root; values are file contents.
    #[serde(default)]
    files: HashMap<String, String>,

    /// Commands sent to clank's stdin.
    stdin: String,

    /// Expected stdout. If absent, stdout is not asserted.
    #[serde(default)]
    stdout: Option<String>,

    /// Expected stderr. If absent, stderr is not asserted.
    #[serde(default)]
    stderr: Option<String>,

    /// Expected config file fields after the session.
    ///
    /// Subset semantics: only the fields listed here are asserted.
    /// Fields present in the written config but absent here are ignored.
    #[serde(default)]
    config_after: Option<toml::Value>,

    /// Expected files in the sandbox after the session.
    ///
    /// Keys are paths relative to sandbox root; values are expected contents.
    #[serde(default)]
    files_after: HashMap<String, String>,
}

// ---------------------------------------------------------------------------
// Test entry point
// ---------------------------------------------------------------------------

#[test]
fn scenario_tests() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let scenarios_dir = manifest_dir.join("tests").join("scenarios");

    // Optional name filter: `cargo test --test scenario -- scenario_tests <filter>`
    let filter: Option<String> = std::env::args()
        .skip_while(|a| a != "scenario_tests")
        .nth(1)
        .filter(|a| !a.starts_with('-'));

    let update_mode = std::env::var("CLANK_UPDATE")
        .map(|v| v == "1")
        .unwrap_or(false);

    let pattern = scenarios_dir
        .join("**")
        .join("*.yaml")
        .to_string_lossy()
        .into_owned();

    let mut paths: Vec<PathBuf> = glob::glob(&pattern)
        .expect("glob pattern failed")
        .flatten()
        .collect();
    paths.sort();

    assert!(
        !paths.is_empty(),
        "no scenario fixtures found under {}",
        scenarios_dir.display()
    );

    let mut failures: Vec<String> = Vec::new();
    let mut ran = 0;
    let mut skipped = 0;

    for path in &paths {
        let name = path
            .strip_prefix(manifest_dir)
            .unwrap_or(path)
            .to_string_lossy()
            .into_owned();

        if let Some(ref f) = filter {
            if !name.contains(f.as_str()) {
                skipped += 1;
                continue;
            }
        }

        ran += 1;
        if update_mode {
            match update_scenario(path) {
                Ok(()) => eprintln!("  updated: {name}"),
                Err(e) => failures.push(format!("{name}\n  {e}")),
            }
        } else {
            match run_scenario(path) {
                Ok(()) => {}
                Err(e) => failures.push(format!("{name}\n  {e}")),
            }
        }
    }

    if skipped > 0 {
        eprintln!("  skipped {skipped} scenario(s) (filter applied)");
    }

    if !failures.is_empty() {
        panic!(
            "{}/{} scenario(s) failed:\n\n{}",
            failures.len(),
            ran,
            failures.join("\n\n---\n\n")
        );
    }
}

// ---------------------------------------------------------------------------
// Scenario runner
// ---------------------------------------------------------------------------

fn run_scenario(path: &Path) -> Result<(), String> {
    let (scenario, sandbox) = prepare_scenario(path)?;
    let config_path = sandbox.path().join("ask.toml");
    let cwd = sandbox.path().to_str().unwrap().to_string();

    let output = spawn_clank(&scenario, &sandbox, &config_path)?;

    let actual_stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let actual_stderr = String::from_utf8_lossy(&output.stderr).into_owned();

    // Assert stdout.
    if let Some(ref expected) = scenario.stdout {
        let expected = expand_tokens(expected, &config_path, &cwd);
        if actual_stdout != expected {
            return Err(format!(
                "stdout mismatch\n--- expected ---\n{expected}--- actual ---\n{actual_stdout}"
            ));
        }
    }

    // Assert stderr.
    if let Some(ref expected) = scenario.stderr {
        let expected = expand_tokens(expected, &config_path, &cwd);
        if actual_stderr != expected {
            return Err(format!(
                "stderr mismatch\n--- expected ---\n{expected}--- actual ---\n{actual_stderr}"
            ));
        }
    }

    // Assert config_after.
    if let Some(ref expected_cfg) = scenario.config_after {
        if !config_path.exists() {
            return Err(
                "config_after specified but no config file was written by the session".to_string(),
            );
        }
        let written = std::fs::read_to_string(&config_path)
            .map_err(|e| format!("could not read config after session: {e}"))?;
        let actual_cfg: toml::Value =
            toml::from_str(&written).map_err(|e| format!("could not parse written config: {e}"))?;
        assert_toml_subset(expected_cfg, &actual_cfg, "")
            .map_err(|e| format!("config_after mismatch: {e}"))?;
    }

    // Assert files_after.
    for (rel_path, expected_contents) in &scenario.files_after {
        let abs = sandbox.path().join(rel_path);
        if !abs.exists() {
            return Err(format!(
                "files_after: expected file not written: {rel_path}"
            ));
        }
        let actual = std::fs::read_to_string(&abs)
            .map_err(|e| format!("files_after: could not read {rel_path}: {e}"))?;
        if &actual != expected_contents {
            return Err(format!(
                "files_after mismatch for {rel_path}\n--- expected ---\n{expected_contents}--- actual ---\n{actual}"
            ));
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Update mode — regenerate stdout/stderr in the fixture file
// ---------------------------------------------------------------------------

fn update_scenario(path: &Path) -> Result<(), String> {
    let (scenario, sandbox) = prepare_scenario(path)?;
    let config_path = sandbox.path().join("ask.toml");

    let output = spawn_clank(&scenario, &sandbox, &config_path)?;

    let actual_stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let actual_stderr = String::from_utf8_lossy(&output.stderr).into_owned();

    // Re-read and re-parse the raw YAML so we can update it in place.
    let src = std::fs::read_to_string(path)
        .map_err(|e| format!("could not read fixture for update: {e}"))?;

    let mut doc: serde_yaml::Value =
        serde_yaml::from_str(&src).map_err(|e| format!("could not parse fixture: {e}"))?;

    let map = doc
        .as_mapping_mut()
        .ok_or("fixture root must be a YAML mapping")?;

    // Only update stdout/stderr if they were already present in the fixture
    // (even as null) or if there is actual output to record. This prevents
    // accidentally adding empty assertions to fixtures that intentionally
    // omit them.
    let had_stdout = map.contains_key("stdout");
    let had_stderr = map.contains_key("stderr");

    if had_stdout || !actual_stdout.is_empty() {
        map.insert(
            serde_yaml::Value::String("stdout".to_string()),
            serde_yaml::Value::String(actual_stdout),
        );
    }
    if had_stderr || !actual_stderr.is_empty() {
        map.insert(
            serde_yaml::Value::String("stderr".to_string()),
            serde_yaml::Value::String(actual_stderr),
        );
    }

    let updated = serde_yaml::to_string(&doc)
        .map_err(|e| format!("could not serialise updated fixture: {e}"))?;

    std::fs::write(path, updated).map_err(|e| format!("could not write updated fixture: {e}"))?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// Prepare a sandbox and pre-populate files; return the parsed scenario and
/// the TempDir (kept alive for the duration of the test).
fn prepare_scenario(path: &Path) -> Result<(Scenario, TempDir), String> {
    let src = std::fs::read_to_string(path).map_err(|e| format!("could not read fixture: {e}"))?;

    let scenario: Scenario =
        serde_yaml::from_str(&src).map_err(|e| format!("could not parse fixture YAML: {e}"))?;

    let sandbox = TempDir::new().map_err(|e| format!("could not create sandbox tempdir: {e}"))?;
    let config_path = sandbox.path().join("ask.toml");

    // Pre-populate config file.
    if let Some(ref cfg) = scenario.config {
        let toml_str = toml::to_string_pretty(cfg)
            .map_err(|e| format!("could not serialise config to TOML: {e}"))?;
        std::fs::write(&config_path, toml_str)
            .map_err(|e| format!("could not write initial config: {e}"))?;
    }

    // Pre-populate arbitrary files.
    for (rel, contents) in &scenario.files {
        let abs = sandbox.path().join(rel);
        if let Some(parent) = abs.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("could not create dir for {rel}: {e}"))?;
        }
        std::fs::write(&abs, contents)
            .map_err(|e| format!("could not write pre-populated file {rel}: {e}"))?;
    }

    Ok((scenario, sandbox))
}

/// Spawn the `clank` binary with the scenario's env and stdin; return the
/// raw `std::process::Output`.
fn spawn_clank(
    scenario: &Scenario,
    sandbox: &TempDir,
    config_path: &Path,
) -> Result<std::process::Output, String> {
    let cwd = sandbox.path().to_str().unwrap().to_string();

    let mut cmd = cargo_bin_cmd!("clank");
    cmd.current_dir(sandbox.path());

    // Always inject CLANK_CONFIG unless the fixture explicitly overrides it.
    let has_explicit_config = scenario.env.contains_key("CLANK_CONFIG");
    if !has_explicit_config {
        cmd.env("CLANK_CONFIG", config_path);
    }

    // Inject fixture env vars, expanding tokens.
    for (k, v) in &scenario.env {
        let expanded = expand_tokens(v, config_path, &cwd);
        cmd.env(k, expanded);
    }

    cmd.write_stdin(scenario.stdin.as_bytes());

    cmd.output()
        .map_err(|e| format!("failed to spawn clank binary: {e}"))
}

/// Expand `{config}` and `{cwd}` tokens in a string.
fn expand_tokens(s: &str, config_path: &Path, cwd: &str) -> String {
    s.replace("{config}", config_path.to_str().unwrap_or(""))
        .replace("{cwd}", cwd)
}

/// Assert that every key present in `expected` exists in `actual` with the
/// same value. Keys present in `actual` but absent in `expected` are ignored.
///
/// `path` is the dot-separated key path for error messages (empty at the root).
fn assert_toml_subset(
    expected: &toml::Value,
    actual: &toml::Value,
    path: &str,
) -> Result<(), String> {
    match (expected, actual) {
        (toml::Value::Table(exp), toml::Value::Table(act)) => {
            for (k, v) in exp {
                let child_path = if path.is_empty() {
                    k.clone()
                } else {
                    format!("{path}.{k}")
                };
                match act.get(k) {
                    None => {
                        return Err(format!("missing key: {child_path}"));
                    }
                    Some(a) => {
                        assert_toml_subset(v, a, &child_path)?;
                    }
                }
            }
            Ok(())
        }
        (e, a) if e == a => Ok(()),
        (e, a) => Err(format!("at {path}: expected {e:?}, got {a:?}")),
    }
}
