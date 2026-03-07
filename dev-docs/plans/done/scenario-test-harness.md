---
title: "Plan: Scenario test harness (YAML fixtures, custom runner)"
date: 2026-03-06
author: agent
issue: "dev-docs/issues/open/scenario-test-harness.md"
---

# Plan: Scenario test harness (YAML fixtures, custom runner)

## Originating Issue

`dev-docs/issues/open/scenario-test-harness.md` — the trycmd-based golden test infrastructure
is fragile, has no config isolation, leaves four fixtures with no output assertions, and does
not extend cleanly to Phase 3 and beyond.

## Approach

Replace `trycmd` and the `.toml`/`.stdout`/`.stderr` split-file format with a custom test
harness and a single-file YAML fixture format. The harness is ~150 lines of Rust built on
`assert_cmd` (already a dev-dependency) and `serde_yaml` (new dev-dependency). Every existing
valid fixture is migrated to the new format, and the previously content-free fixtures gain
real output assertions.

---

## Fixture format

Each test case is a single `.yaml` file. All fields are optional except `stdin`.

```yaml
# Minimum valid fixture — only stdin required
stdin: |
  echo "hello world"
stdout: |
  $ hello world
  $ 
```

### Full schema

```yaml
# Human-readable description of what this test covers.
# Shown in test output on failure.
desc: "optional description"

# Environment variables injected into the clank process.
# The special token {config} expands to the path of the isolated config file
# for this test. {cwd} expands to the sandbox directory path.
# Any key set here overrides the inherited environment.
env:
  CLANK_CONFIG: "{config}"
  MY_VAR: "some value"

# Initial config file contents, expressed as a TOML structure.
# If present, this is serialised to the isolated config path before the test runs.
# If absent (or null), no config file exists before the test (tests the no-config path).
config:
  default_model: "ollama/llama3.2"
  providers:
    ollama:
      base_url: "http://localhost:11434"

# Files to pre-populate in the sandbox directory before the test runs.
# Keys are paths relative to the sandbox root; values are file contents.
files:
  "scripts/hello.sh": |
    #!/bin/sh
    echo hello

# Commands sent to clank's stdin. This is the full session transcript.
# Use literal YAML block scalar (|) for multi-line sessions.
stdin: |
  model add ollama
  model list

# Expected stdout. Must match exactly (byte-for-byte after newline normalisation).
# Omit or set to null to skip stdout assertion.
stdout: |
  $ Provider 'ollama' configured.
  $ Default model: anthropic/claude-sonnet-4-5
  
  Providers:
    ollama: base_url=http://localhost:11434
  $ 

# Expected stderr. Same rules as stdout.
# Omit or set to null to skip stderr assertion.
stderr: ""

# Expected config file contents after the session, as a TOML structure.
# The harness reads the config file, deserialises it, and asserts field by field.
# Only the fields listed here are asserted — unspecified fields are ignored.
# Omit to skip config file assertion.
config_after:
  providers:
    ollama:
      base_url: "http://localhost:11434"

# Expected files in the sandbox after the session.
# Keys are paths relative to sandbox root; values are expected file contents.
# Omit to skip filesystem assertions.
files_after:
  "out.txt": "hello\n"
```

### Token expansion

The following tokens are expanded in `env` values and in `stdout`/`stderr` expected strings:

| Token | Expands to |
|---|---|
| `{config}` | Absolute path of the isolated config TOML file in the sandbox |
| `{cwd}` | Absolute path of the sandbox directory |

Token expansion allows tests to assert on paths that vary per run. Example:

```yaml
env:
  CLANK_CONFIG: "{config}"
stdout: |
  $ Config written to {config}
  $ 
```

### Omission semantics

- `stdout: null` or absent → stdout is not asserted (any value accepted)
- `stderr: null` or absent → stderr is not asserted (any value accepted)
- `config: null` or absent → no config file is written before the test
- `config_after: null` or absent → config file is not read or asserted after the test
- `env` absent → `CLANK_CONFIG` is still set to `{config}` (see isolation section below)

---

## Isolation guarantee

**Every test run gets an isolated config file path by default.** The harness always:

1. Creates a `tempfile::TempDir` for the test.
2. Sets `CLANK_CONFIG` to `<tempdir>/ask.toml` — unless the fixture's `env` table explicitly
   overrides `CLANK_CONFIG` with a different value.
3. If `config:` is specified, serialises it to `<tempdir>/ask.toml` before spawning the
   process.

This means:
- Fixtures that do not mention `config:` always start with no config file. There is no
  implicit dependency on the developer's `~/.config/ask/ask.toml`.
- Fixtures that write config (via `model add`) have their writes sandboxed to the tempdir.
- The isolation requires no action from the fixture author — it is the default.

Fixtures that intentionally need to inherit the real user config (none exist today) can opt
out by setting `env: {CLANK_CONFIG: ""}` explicitly.

---

## The harness

### Location

`crates/clank/tests/scenario.rs` — a new Level 3 test file alongside the existing
`ask.rs`, `shell_basics.rs`, and (after migration) the deleted `golden.rs`.

### Structure

```rust
// crates/clank/tests/scenario.rs

use std::path::Path;
use assert_cmd::cargo_bin_cmd;
use tempfile::TempDir;

/// Scenario test runner. Reads all *.yaml files under tests/scenarios/,
/// runs each as a clank process, and asserts on stdout, stderr, and
/// resulting filesystem state.
#[test]
fn scenario_tests() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let fixtures_dir = manifest_dir.join("tests").join("scenarios");

    let mut failures = Vec::new();
    let mut count = 0;

    for entry in glob::glob(fixtures_dir.join("**/*.yaml").to_str().unwrap())
        .expect("glob failed")
        .flatten()
    {
        count += 1;
        if let Err(e) = run_scenario(&entry) {
            failures.push(format!("{}: {}", entry.display(), e));
        }
    }

    if !failures.is_empty() {
        panic!("{}/{} scenario(s) failed:\n{}", failures.len(), count, failures.join("\n---\n"));
    }
}

fn run_scenario(path: &Path) -> Result<(), String> {
    let src = std::fs::read_to_string(path)
        .map_err(|e| format!("could not read fixture: {e}"))?;
    let scenario: Scenario = serde_yaml::from_str(&src)
        .map_err(|e| format!("could not parse fixture: {e}"))?;

    let dir = TempDir::new().map_err(|e| format!("tempdir failed: {e}"))?;
    let config_path = dir.path().join("ask.toml");
    let cwd = dir.path().to_str().unwrap().to_string();

    // Pre-populate config file if specified.
    if let Some(ref cfg) = scenario.config {
        let toml = toml::to_string_pretty(cfg)
            .map_err(|e| format!("could not serialise config: {e}"))?;
        std::fs::write(&config_path, toml)
            .map_err(|e| format!("could not write config: {e}"))?;
    }

    // Pre-populate files if specified.
    for (rel_path, contents) in &scenario.files {
        let abs = dir.path().join(rel_path);
        if let Some(parent) = abs.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("could not create dir: {e}"))?;
        }
        std::fs::write(&abs, contents)
            .map_err(|e| format!("could not write file: {e}"))?;
    }

    // Build env: always inject CLANK_CONFIG unless fixture overrides it.
    let default_clank_config = format!("{config_path_str}", config_path_str = config_path.to_str().unwrap());
    let mut env: Vec<(String, String)> = vec![];
    let has_explicit_config = scenario.env.contains_key("CLANK_CONFIG");
    if !has_explicit_config {
        env.push(("CLANK_CONFIG".to_string(), default_clank_config.clone()));
    }
    for (k, v) in &scenario.env {
        let expanded = v
            .replace("{config}", config_path.to_str().unwrap())
            .replace("{cwd}", &cwd);
        env.push((k.clone(), expanded));
    }

    // Run the binary.
    let mut cmd = cargo_bin_cmd!("clank");
    for (k, v) in &env {
        cmd.env(k, v);
    }
    cmd.current_dir(dir.path());
    cmd.write_stdin(scenario.stdin.as_bytes());

    let output = cmd.output().map_err(|e| format!("failed to spawn clank: {e}"))?;
    let actual_stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let actual_stderr = String::from_utf8_lossy(&output.stderr).into_owned();

    // Assert stdout.
    if let Some(ref expected) = scenario.stdout {
        let expected = expected
            .replace("{config}", config_path.to_str().unwrap())
            .replace("{cwd}", &cwd);
        if actual_stdout != expected {
            return Err(format!(
                "stdout mismatch\n--- expected ---\n{expected}\n--- actual ---\n{actual_stdout}"
            ));
        }
    }

    // Assert stderr.
    if let Some(ref expected) = scenario.stderr {
        let expected = expected
            .replace("{config}", config_path.to_str().unwrap())
            .replace("{cwd}", &cwd);
        if actual_stderr != expected {
            return Err(format!(
                "stderr mismatch\n--- expected ---\n{expected}\n--- actual ---\n{actual_stderr}"
            ));
        }
    }

    // Assert config_after.
    if let Some(ref expected_cfg) = scenario.config_after {
        if !config_path.exists() {
            return Err("config_after specified but no config file was written".to_string());
        }
        let written = std::fs::read_to_string(&config_path)
            .map_err(|e| format!("could not read config after: {e}"))?;
        let actual_cfg: toml::Value = toml::from_str(&written)
            .map_err(|e| format!("could not parse written config: {e}"))?;
        assert_toml_subset(expected_cfg, &actual_cfg)
            .map_err(|e| format!("config_after mismatch: {e}"))?;
    }

    // Assert files_after.
    for (rel_path, expected_contents) in &scenario.files_after {
        let abs = dir.path().join(rel_path);
        if !abs.exists() {
            return Err(format!("files_after: expected file not found: {rel_path}"));
        }
        let actual = std::fs::read_to_string(&abs)
            .map_err(|e| format!("files_after: could not read {rel_path}: {e}"))?;
        if &actual != expected_contents {
            return Err(format!(
                "files_after mismatch for {rel_path}\n--- expected ---\n{expected_contents}\n--- actual ---\n{actual}"
            ));
        }
    }

    Ok(())
}
```

### Key types

```rust
#[derive(Deserialize)]
struct Scenario {
    #[serde(default)]
    desc: Option<String>,
    #[serde(default)]
    env: std::collections::HashMap<String, String>,
    #[serde(default)]
    config: Option<toml::Value>,
    #[serde(default)]
    files: std::collections::HashMap<String, String>,
    stdin: String,
    #[serde(default)]
    stdout: Option<String>,
    #[serde(default)]
    stderr: Option<String>,
    #[serde(default)]
    config_after: Option<toml::Value>,
    #[serde(default)]
    files_after: std::collections::HashMap<String, String>,
}
```

### `assert_toml_subset`

```rust
/// Assert that every key present in `expected` exists in `actual` with the same value.
/// Keys present in `actual` but absent in `expected` are ignored.
fn assert_toml_subset(expected: &toml::Value, actual: &toml::Value) -> Result<(), String> {
    match (expected, actual) {
        (toml::Value::Table(exp), toml::Value::Table(act)) => {
            for (k, v) in exp {
                match act.get(k) {
                    None => return Err(format!("missing key: {k}")),
                    Some(a) => assert_toml_subset(v, a)
                        .map_err(|e| format!("{k}.{e}"))?,
                }
            }
            Ok(())
        }
        (e, a) if e == a => Ok(()),
        (e, a) => Err(format!("expected {e:?}, got {a:?}")),
    }
}
```

### Regeneration

When `CLANK_UPDATE=1` is set, the harness writes the actual stdout/stderr back to the fixture
file instead of asserting. This is the equivalent of `TRYCMD=overwrite`:

```sh
CLANK_UPDATE=1 cargo test --test scenario
```

The harness detects `CLANK_UPDATE=1`, runs the binary, and updates the `stdout:` and
`stderr:` fields in-place using a YAML-aware writer. Fields that were previously `null` gain
values; fields that were already set are overwritten.

---

## New fixture location

Fixtures move from `tests/fixtures/` to `tests/scenarios/`. The subdirectory structure is
preserved:

```
tests/scenarios/
  shell_basics/
    echo.yaml
    and_short_circuit.yaml
    or_on_failure.yaml
    semicolon_sequence.yaml
    export_and_expand.yaml
    multiline_buffering.yaml
  ask/
    ask_stub.yaml
    ask_no_config.yaml
    context_clear.yaml
    context_show_empty.yaml
    model_list_no_config.yaml
  model/
    add_ollama_default.yaml
    add_openai_compat_missing_url.yaml
    list_with_local_providers.yaml
```

---

## Migrated fixtures

### `shell_basics/echo.yaml`

```yaml
desc: "echo produces output on stdout prefixed with the shell prompt"
stdin: |
  echo "hello world"
stdout: |
  $ hello world
  $ 
```

### `shell_basics/and_short_circuit.yaml`

```yaml
desc: "false && echo does not print when left side fails"
stdin: |
  false && echo "should not print"
stdout: "$ $ \n"
```

### `shell_basics/or_on_failure.yaml`

```yaml
desc: "false || echo runs right side when left side fails"
stdin: |
  false || echo "ran"
stdout: |
  $ ran
  $ 
```

### `shell_basics/semicolon_sequence.yaml`

```yaml
desc: "false ; echo always runs right side regardless of exit code"
stdin: |
  false ; echo "ran"
stdout: |
  $ ran
  $ 
```

### `shell_basics/export_and_expand.yaml`

```yaml
desc: "exported variable is visible to subsequent commands in the same session"
stdin: |
  export FOO=bar && echo $FOO
stdout: |
  $ bar
  $ 
```

### `shell_basics/multiline_buffering.yaml`

```yaml
desc: "multi-line if/then/fi is buffered and executed correctly"
stdin: |
  if true; then
  echo "multiline"
  fi
stdout: |
  $ > > multiline
  $ 
```

### `ask/ask_stub.yaml`

```yaml
desc: "ask with no API key configured prints an informative error to stderr"
stdin: |
  ask "hello"
stdout: "$ $ \n"
stderr: |
  clank: ask: no API key configured for provider 'anthropic' or 'openrouter'
  To use Anthropic directly:  add [providers.anthropic] api_key = "..." to ~/.config/ask/ask.toml
  To use OpenRouter:          add [providers.openrouter] api_key = "..." to ~/.config/ask/ask.toml
```

### `ask/ask_no_config.yaml`

```yaml
desc: "ask with no config file prints the same no-API-key error"
stdin: |
  ask "hello"
stdout: "$ $ \n"
stderr: |
  clank: ask: no API key configured for provider 'anthropic' or 'openrouter'
  To use Anthropic directly:  add [providers.anthropic] api_key = "..." to ~/.config/ask/ask.toml
  To use OpenRouter:          add [providers.openrouter] api_key = "..." to ~/.config/ask/ask.toml
```

### `ask/context_clear.yaml`

```yaml
desc: "context clear succeeds and produces no output"
stdin: |
  context clear
stdout: "$ $ \n"
stderr: ""
```

### `ask/context_show_empty.yaml`

```yaml
desc: "context show with an empty transcript reports that context is empty"
stdin: |
  context show
stdout: |
  $ [context is empty]
  $ 
stderr: ""
```

Note: the exact stdout for `context show` must be confirmed by running `CLANK_UPDATE=1` — the
placeholder `[context is empty]` above is illustrative. The harness will capture the real
output on first run.

### `ask/model_list_no_config.yaml`

```yaml
desc: "model list with no config file prints the no-providers message"
stdin: |
  model list
stdout: |
  $ No providers configured.
  $ Run: model add anthropic --key <KEY>
  $ 
stderr: ""
```

### `model/add_ollama_default.yaml`

```yaml
desc: "model add ollama with no --url writes the default base_url"
stdin: |
  model add ollama
stdout: |
  $ Provider 'ollama' configured.
  $ 
stderr: ""
config_after:
  providers:
    ollama:
      base_url: "http://localhost:11434"
```

### `model/add_openai_compat_missing_url.yaml`

```yaml
desc: "model add openai-compat without --url exits with usage error on stderr"
stdin: |
  model add openai-compat
stdout: "$ $ \n"
stderr: |
  clank: model add: --url is required for provider 'openai-compat'
  usage: model add openai-compat --url <BASE_URL> [--key <API_KEY>]
```

### `model/list_with_local_providers.yaml`

```yaml
desc: "model list after adding ollama and openai-compat shows both with base_url"
stdin: |
  model add ollama
  model add openai-compat --url http://localhost:8080
  model list
stdout: |
  $ Provider 'ollama' configured.
  $ Provider 'openai-compat' configured.
  $ Default model: anthropic/claude-sonnet-4-5

  Providers:
    ollama: base_url=http://localhost:11434
    openai-compat: base_url=http://localhost:8080
  $ 
stderr: ""
config_after:
  providers:
    ollama:
      base_url: "http://localhost:11434"
    openai-compat:
      base_url: "http://localhost:8080"
```

---

## Dependencies

Add to `crates/clank/Cargo.toml` `[dev-dependencies]`:

```toml
serde_yaml = "0.9"
glob = "0.3"
tempfile = "3"
toml = { workspace = true }
serde = { version = "1", features = ["derive"] }
```

Remove from `[workspace.dependencies]` (and from any crate that only used it for tests):

```toml
trycmd = "1.0"   # remove
```

---

## What happens to `golden.rs`, `tests/fixtures/`, and trycmd

1. `crates/clank/tests/golden.rs` is deleted.
2. `crates/clank/tests/fixtures/` is deleted (all content migrated to `tests/scenarios/`).
3. `trycmd` is removed from `[workspace.dependencies]` and from `crates/clank/Cargo.toml`.
4. `crates/clank/tests/ask.rs` is updated: `test_model_list_no_config` gains explicit
   `CLANK_CONFIG` isolation (now that the pattern is established). `has_api_key()` is
   updated to check `CLANK_CONFIG` env var first, then the platform default, consistent with
   the binary's own logic.
5. `crates/clank/tests/shell_basics.rs` is updated: `test_ask_no_config` gains explicit
   `CLANK_CONFIG` isolation.

---

## Acceptance tests

The acceptance criteria for this plan are:

1. `cargo test --test scenario` passes with zero failures.
2. All 14 currently-passing trycmd cases are covered by scenario fixtures.
3. The four previously content-free `ask/` fixtures now assert on stdout and stderr.
4. `CLANK_UPDATE=1 cargo test --test scenario` regenerates fixture files without changing
   any fixture whose output has not changed.
5. `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check` pass.
6. `trycmd` is removed from the workspace dependencies and does not appear in `Cargo.lock`.

---

## Tasks

- [ ] **H1** Add `serde_yaml`, `glob`, `tempfile`, `toml`, `serde` to `crates/clank/[dev-dependencies]`
- [ ] **H2** Write `crates/clank/tests/scenario.rs`: `Scenario` struct, `run_scenario()`, `assert_toml_subset()`, `scenario_tests()` entry point
- [ ] **H3** Implement `CLANK_UPDATE=1` regeneration mode in the harness
- [ ] **M1** Create `tests/scenarios/` directory structure; migrate all 14 existing trycmd fixtures to YAML; confirm all pass
- [ ] **M2** Add stdout/stderr assertions to the four previously content-free `ask/` fixtures; confirm output with `CLANK_UPDATE=1` then lock in
- [ ] **M3** Add `config_after:` assertions to `model/add_ollama_default.yaml` and `model/list_with_local_providers.yaml`
- [ ] **C1** Delete `crates/clank/tests/golden.rs` and `tests/fixtures/`
- [ ] **C2** Remove `trycmd` from `[workspace.dependencies]` and `crates/clank/Cargo.toml`
- [ ] **C3** Update `crates/clank/tests/ask.rs`: add `CLANK_CONFIG` isolation to `test_model_list_no_config`; fix `has_api_key()` to check `CLANK_CONFIG` env var first
- [ ] **C4** Update `crates/clank/tests/shell_basics.rs`: add `CLANK_CONFIG` isolation to `test_ask_no_config`
- [ ] **QG** `cargo test --workspace`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check` all pass; `trycmd` absent from `Cargo.lock`
