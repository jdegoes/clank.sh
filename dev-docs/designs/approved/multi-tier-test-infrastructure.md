---
title: "Multi-Tier Test Infrastructure"
date: 2026-03-06
author: agent
---

# Multi-Tier Test Infrastructure

## Purpose

This document specifies the full design of clank's three-tier test infrastructure: unit tests, integration tests, and acceptance tests. It is the reference for implementation and for future contributors adding test cases or extending the harness.

---

## 1. Workspace Structure

After this change the workspace members are:

```
clank-core/          # library crate — shell logic; owns tiers 1 and 2
clank-shell/         # binary crate — thin entry point; owns nothing testable directly
clank-http/          # library crate — HTTP abstraction; unchanged
clank-acceptance/    # test-only crate — tier 3 acceptance harness
```

Root `Cargo.toml` workspace members list:

```toml
[workspace]
members  = ["clank-core", "clank-shell", "clank-http", "clank-acceptance"]
resolver = "2"
```

---

## 2. `clank-core` Crate

### Purpose

All reusable shell logic lives here. It is a library crate. The `clank-shell` binary, integration tests, and any future embedder all depend on `clank-core`.

### Directory layout

```
clank-core/
  Cargo.toml
  src/
    lib.rs
  tests/
    shell_builder.rs    # tier 2 integration tests
```

### `Cargo.toml`

```toml
[package]
name    = "clank-core"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
brush-core     = "0.4"
brush-parser   = "0.3"
brush-builtins = "0.1"
tokio          = { version = "1", features = ["full"] }

[dev-dependencies]
tokio = { version = "1", features = ["rt", "macros"] }
```

### `src/lib.rs` public API

```rust
pub use brush_core::{CreateOptions, Shell};

/// Default `CreateOptions` for a non-interactive clank shell instance.
pub fn default_options() -> CreateOptions

/// Boot a shell with the given options and execute a single command string.
/// Returns the numeric exit code (0–255).
pub async fn run_with_options(
    command: &str,
    options: CreateOptions,
) -> Result<u8, brush_core::Error>

/// Boot a shell with default non-interactive options and execute a command.
/// Convenience wrapper around `run_with_options`.
pub async fn run(command: &str) -> Result<u8, brush_core::Error>
```

`default_options()` returns:

```rust
CreateOptions {
    interactive:  false,
    no_profile:   true,
    no_rc:        true,
    no_editing:   true,
    shell_name:   Some("clank".to_owned()),
    ..CreateOptions::default()
}
```

Exposing `default_options()` and `run_with_options()` separately allows integration tests to precisely vary individual `CreateOptions` fields without reimplementing the defaults.

### Tier 1: unit tests (in `src/lib.rs`)

Inline `#[cfg(test)]` block. Tests that exercise small, synchronous aspects of the library (e.g., `default_options()` field values). The two existing tests from `clank-shell/src/main.rs` move here:

```rust
#[tokio::test]
async fn echo_hello_exits_zero()

#[tokio::test]
async fn false_exits_nonzero()
```

---

## 3. `clank-shell` Crate (Simplified)

### Purpose

A thin binary wrapper. Contains only `src/main.rs`. No library target. No tests.

### `Cargo.toml`

```toml
[package]
name    = "clank-shell"
version.workspace = true
edition.workspace = true
license.workspace = true

[[bin]]
name = "clank"
path = "src/main.rs"

[dependencies]
clank-core = { path = "../clank-core" }
tokio      = { version = "1", features = ["full"] }
```

All brush-core imports are removed from `clank-shell`. It depends only on `clank-core` and `tokio`.

### `src/main.rs` behaviour

Two modes, determined at startup:

1. **Argv mode:** one or more arguments are provided. The arguments are joined with spaces and executed as a command string. (Current behaviour, preserved.)
2. **Stdin mode:** no arguments provided. The binary reads all of stdin to a string and executes that. This is required by the acceptance tier, which pipes YAML-extracted script bodies via stdin.

```
clank "echo hello"          # argv mode
echo "echo hello" | clank   # stdin mode
```

In stdin mode, if stdin is empty the shell executes an empty string and exits 0 (Brush's behaviour for an empty program).

---

## 4. Tier 2: Integration Tests (`clank-core/tests/`)

### File: `clank-core/tests/shell_builder.rs`

Each function is a standard `#[tokio::test]`. The tests use `clank_core::` public API only.

### Test cases

| Test name | What it validates |
|---|---|
| `default_options_is_non_interactive` | `default_options().interactive == false` |
| `default_options_skips_profile` | `default_options().no_profile == true` |
| `default_options_skips_rc` | `default_options().no_rc == true` |
| `default_options_shell_name_is_clank` | `default_options().shell_name == Some("clank")` |
| `run_echo_returns_zero` | `run("echo hello")` → exit code 0 |
| `run_false_returns_nonzero` | `run("false")` → exit code 1 |
| `run_exit_42_returns_42` | `run("exit 42")` → exit code 42 |
| `run_with_options_respects_custom_name` | `run_with_options("echo $0", opts_with_name("myshell"))` → stdout contains `myshell` |
| `shell_new_with_default_options_succeeds` | `Shell::new(default_options()).await` completes without error |

---

## 5. Tier 3: Acceptance Tests (`clank-acceptance/`)

### Purpose

Black-box end-to-end tests. The `clank` binary is spawned as a subprocess. Tests assert on stdout, stderr, and exit code only — they have no access to internals.

### Directory layout

```
clank-acceptance/
  Cargo.toml
  tests/
    acceptance.rs         # datatest-stable harness entry point
  cases/
    scripting/
      pipelines.yaml
    exit_codes/
      basic.yaml
    builtins/
      alias.yaml
```

`cases/` is the fixture root. Subdirectory names group related cases by topic. Each `.yaml` file is a test suite (one `name`, one `cases` list). The harness discovers all `.yaml` files under `cases/` recursively. Adding a new `.yaml` file requires no code changes.

### `Cargo.toml`

```toml
[package]
name    = "clank-acceptance"
version.workspace = true
edition.workspace = true
license.workspace = true
publish = false

# Forces `clank` binary to be compiled before tests run,
# and makes CARGO_BIN_EXE_clank available at compile time.
[dev-dependencies]
clank-shell     = { path = "../clank-shell" }
assert_cmd      = "2.1"
predicates      = "3.1"
datatest-stable = "0.3"
serde           = { version = "1", features = ["derive"] }
serde_yaml      = "0.9"

[[test]]
name    = "acceptance"
path    = "tests/acceptance.rs"
harness = false
```

`publish = false` — this crate is never published to crates.io.

The `clank-shell` dev-dependency has no API surface used by the test code; its sole purpose is to instruct Cargo to build the `clank` binary before tests run, and to set `CARGO_BIN_EXE_clank` at compile time.

### YAML test case schema

#### `TestSuite` (top-level structure of each `.yaml` file)

```rust
#[derive(Deserialize)]
struct TestSuite {
    name: String,
    cases: Vec<TestCase>,
}
```

#### `TestCase`

```rust
#[derive(Deserialize)]
struct TestCase {
    /// Short description — used as the test identifier in output.
    name: String,

    /// Shell script source. Passed to clank via stdin.
    stdin: String,

    /// Extra command-line arguments passed to clank (default: none).
    #[serde(default)]
    args: Vec<String>,

    /// Extra environment variables (default: inherit from test process).
    #[serde(default)]
    env: HashMap<String, String>,

    /// Expected exit code (default: 0).
    #[serde(default)]
    expect_exit: u8,

    /// Expected exact stdout, including trailing newline (default: not checked).
    expect_stdout: Option<String>,

    /// Substring that must appear in stdout (default: not checked).
    expect_stdout_contains: Option<String>,

    /// If true, assert stderr is empty (default: false — stderr not checked).
    #[serde(default)]
    expect_stderr_empty: bool,

    /// If true, the test is run but a failure is not fatal (analogous to #[ignore]).
    #[serde(default)]
    known_failure: bool,

    /// If true, the test is not run at all.
    #[serde(default)]
    skip: bool,
}
```

All fields not marked `required` default to their zero value via `#[serde(default)]`. This keeps YAML files minimal — only deviate from defaults when needed.

#### Example YAML files

**`cases/scripting/pipelines.yaml`**:
```yaml
name: "Scripting — pipelines"
cases:
  - name: echo_basic
    stdin: |
      echo "hello world"
    expect_exit: 0
    expect_stdout: "hello world\n"
    expect_stderr_empty: true

  - name: pipeline_exit_code_from_last
    stdin: |
      true | false
    expect_exit: 1
```

**`cases/exit_codes/basic.yaml`**:
```yaml
name: "Exit codes — basic"
cases:
  - name: true_exits_zero
    stdin: "true"
    expect_exit: 0
    expect_stderr_empty: true

  - name: false_exits_one
    stdin: "false"
    expect_exit: 1

  - name: explicit_exit_42
    stdin: "exit 42"
    expect_exit: 42
```

**`cases/builtins/alias.yaml`**:
```yaml
name: "Builtins — alias"
cases:
  - name: alias_define_and_invoke
    stdin: |
      alias greet='echo alias-works'
      greet
    expect_exit: 0
    expect_stdout: "alias-works\n"
    expect_stderr_empty: true
```

### `tests/acceptance.rs` — harness design

```rust
// Entry point registered with datatest-stable.
// Called once per .yaml file found under cases/.
fn run_suite(path: &Path) -> datatest_stable::Result<()> {
    let contents = std::fs::read_to_string(path)?;
    let suite: TestSuite = serde_yaml::from_str(&contents)?;
    for case in suite.cases {
        run_case(&suite.name, &case)?;
    }
    Ok(())
}

fn run_case(suite_name: &str, case: &TestCase) -> datatest_stable::Result<()> {
    if case.skip {
        return Ok(());
    }

    let mut cmd = Command::cargo_bin("clank").unwrap();
    cmd.write_stdin(case.stdin.as_bytes());
    for arg in &case.args {
        cmd.arg(arg);
    }
    for (k, v) in &case.env {
        cmd.env(k, v);
    }

    let output = cmd.output()?;
    let result = assert_case(case, &output);

    if case.known_failure {
        // Invert: a known failure passing is also notable, but not fatal.
        // Log but do not propagate error.
        let _ = result;
        return Ok(());
    }

    result
}

fn assert_case(case: &TestCase, output: &Output) -> datatest_stable::Result<()> {
    // exit code
    assert_eq!(output.status.code(), Some(case.expect_exit as i32), ...);

    // stdout
    if let Some(expected) = &case.expect_stdout {
        assert_eq!(String::from_utf8_lossy(&output.stdout).as_ref(), expected, ...);
    }
    if let Some(contains) = &case.expect_stdout_contains {
        assert!(String::from_utf8_lossy(&output.stdout).contains(contains.as_str()), ...);
    }

    // stderr
    if case.expect_stderr_empty {
        assert!(output.stderr.is_empty(), ...);
    }

    Ok(())
}

datatest_stable::harness! {
    {
        test    = run_suite,
        root    = "cases",
        pattern = r".*\.yaml$",
    },
}
```

**Note on `datatest-stable` and per-case naming:** `datatest-stable` calls `run_suite` once per file, not once per case. Each invocation is a single named test in the output (named after the `.yaml` file path). Per-case granularity within a suite is visible in the error output when a case fails (via the suite name + case name in the assertion message), but individual cases do not appear as separate test entries in `cargo test` output. This is a known limitation; `cargo nextest` gives the same granularity since the unit of parallelism is the file, not the case. If per-case test isolation becomes important in future, each case can be split into its own `.yaml` file.

### Error message format

Assertion failures include the suite name, case name, and the specific field that failed:

```
FAILED [exit_codes/basic.yaml] "Exit codes — basic" / "false_exits_one"
  exit code: got 0, expected 1
```

This is produced by including the context in the `assert_eq!` / `assert!` message argument.

---

## 6. Running Tests

### Standard `cargo test`

```sh
# All tiers:
cargo test --workspace

# Tier 1 + 2 only (no subprocess):
cargo test -p clank-core

# Tier 3 only (requires clank binary to be built):
cargo test -p clank-acceptance

# Including network-gated tests:
cargo test --workspace -- --include-ignored
```

### `cargo nextest`

```sh
cargo nextest run --workspace
```

Each `.yaml` file under `cases/` is a separate test entry. Tests in different files run in parallel automatically.

### Building clank before running acceptance tests

`cargo test -p clank-acceptance` automatically builds the `clank` binary because `clank-shell` is listed as a dev-dependency. No manual `cargo build` step is required.

---

## 7. Design Constraints and Decisions

### Why a separate `clank-acceptance` crate rather than `clank-shell/tests/`?

Placing acceptance tests in `clank-shell/tests/` would work but conflates the concerns of "binary entry point" and "test harness". A separate crate makes the dependency graph explicit (`clank-acceptance` → `clank-shell` for the binary; never the other way), keeps `clank-shell` minimal, and allows the acceptance crate to evolve independently (e.g., adding an oracle tier later without touching `clank-shell`).

### Why YAML rather than annotated `.sh` files?

YAML allows expressing richer metadata per case (multiple assertions, `known_failure`, `skip`, `env`, `args`) in a format that is easy to deserialize with `serde`. Annotated `.sh` files require a custom parser and cannot naturally express multi-field structured assertions without becoming unwieldy. YAML is also the format used by Brush, which makes the case format familiar to anyone studying both projects.

### Why `stdin` rather than `-c` argv for script input?

The acceptance tier passes entire scripts, which may be multi-line. Passing multi-line scripts via `-c` requires careful quoting. Stdin is the natural channel for script files and avoids any shell-quoting interaction between the test harness and the binary under test.

### Why `datatest-stable` rather than a hand-written walker?

`datatest-stable` provides `cargo nextest` integration out of the box. Each `.yaml` file becomes a separately named, parallelisable test without any custom parallelism code. A hand-written walker with `#[test]` would be sequential or require `tokio::test` + manual parallelism.

### Why `clank-shell` as a dev-dependency of `clank-acceptance`?

This is the only mechanism Cargo provides to guarantee: (a) the `clank` binary is compiled before tests run, and (b) `CARGO_BIN_EXE_clank` is set at compile time for `env!("CARGO_BIN_EXE_clank")` in `assert_cmd`. Without this dependency, the variable is not set and `Command::cargo_bin("clank")` fails.

### `known_failure` semantics

A `known_failure` case is run but its result is not propagated as a test failure. This matches the intent of Brush's same field: it documents known divergences from expected behaviour that are not yet fixed, without silently hiding them. The case still runs — a `known_failure` case that starts passing is visible in output as a no-op (the error is swallowed), which is acceptable at this stage. A future enhancement could invert this (fail if a known_failure unexpectedly passes — analogous to `#[should_panic]`).

---

## 8. Future Extensions (Out of Scope for This Issue)

- **Oracle mode:** spawn bash alongside clank, diff outputs. Requires bash on the test host. Tracked as a separate issue once expectations-only is stable.
- **Per-case nextest granularity:** split large YAML suites or use a custom datatest-stable extension that registers one test per case.
- **Temp directory isolation:** each acceptance test case runs in the process's working directory. Future cases involving filesystem operations will need a `tempdir` per case.
- **Timeout per case:** `TestCase` schema has a `timeout_in_seconds` field in Brush's design; deferred here.
- **PTY testing:** for interactive shell behaviour. Not relevant until `clank` has an interactive REPL mode.
