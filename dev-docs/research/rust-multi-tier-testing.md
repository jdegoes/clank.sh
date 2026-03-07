---
title: "Rust Multi-Tier Testing in a Cargo Workspace"
date: 2026-03-06
author: agent
---

## Scope

This document covers four questions required to design clank's test infrastructure:

1. How Cargo integration tests work and what is required to add a `[lib]` target alongside `[[bin]]`.
2. The idiomatic pattern for a separate acceptance-test crate in a workspace.
3. File-driven test case discovery with `datatest-stable`.
4. How brush-shell structures its oracle/compatibility tests and what can be adapted.

---

## 1. Cargo Integration Tests and the `[lib]` + `[[bin]]` Pattern

### How `tests/` works

Files placed under `tests/` at the package root are *integration tests*. Cargo compiles each file as a separate crate (not a module of the main crate), linked against the package's **library** target. Each file may contain `#[test]` functions in the normal way.

Key mechanics:

- Integration tests can only call functions in the package's **public API** — they are external consumers.
- They are linked with `[dependencies]` and `[dev-dependencies]` from `Cargo.toml`.
- Cargo runs them automatically with `cargo test`. They are listed in output as a separate binary per file.
- The working directory when each test runs is the package root (the directory containing `Cargo.toml`).
- Shared code used by multiple test files must go into `tests/common/mod.rs` (the old convention) or `tests/common/` with a `mod.rs`; a file `tests/common.rs` would be compiled as its own test binary, which is almost never what you want.

### Requiring a `[lib]` alongside `[[bin]]`

Integration tests in `tests/` link against the *library* target (`src/lib.rs` by default). If a package has **only** a `[[bin]]` target (i.e., `src/main.rs` and no `src/lib.rs`), Cargo will not produce a library artifact, and nothing in `tests/` can `use` code from `src/`.

To expose library API alongside a binary:

```toml
# Cargo.toml

[lib]
name = "clank"          # becomes `libclank.rlib`
path = "src/lib.rs"

[[bin]]
name = "clank"
path = "src/main.rs"    # typically: `fn main() { clank::run(); }`
```

`src/main.rs` then re-exports or calls into `src/lib.rs`. The integration tests `use clank::...` as any downstream consumer would.

**Gotchas when transitioning from bin-only:**

- `src/main.rs` can no longer define modules that integration tests need — those must move to `src/lib.rs`.
- If any type or function in `src/main.rs` is referenced from tests, it must move to the library.
- `fn main()` is special: it cannot be tested via the library path. CLI entry points should be thin wrappers that call into the library.
- Cargo will detect both targets via auto-discovery as long as both `src/lib.rs` and `src/main.rs` exist; explicit `[lib]`/`[[bin]]` sections are optional but clarify intent and allow custom paths.
- `CARGO_BIN_EXE_<name>` (see section 2) is only set for integration tests, not unit tests. This is a frequent source of confusion when a test is mistakenly placed in `src/` rather than `tests/`.

### `[[test]]` table

A custom test target can be registered explicitly:

```toml
[[test]]
name = "my-test"
path = "tests/my_test.rs"
harness = false           # opt out of libtest; the file provides its own main()
```

`harness = false` is required when using a custom test runner (e.g., `datatest-stable`, a tokio-based main, or a fully custom harness). Without it Cargo wraps the file with libtest's `main()`.

---

## 2. Acceptance Test Crate in a Workspace

### Workspace layout

The idiomatic pattern for subprocess-based acceptance tests is a dedicated workspace member whose sole purpose is testing:

```
workspace/
├── Cargo.toml           # [workspace] members = ["clank", "clank-acceptance-tests"]
├── clank/
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       └── main.rs
└── clank-acceptance-tests/
    ├── Cargo.toml
    └── tests/
        └── acceptance.rs
```

The acceptance crate's `Cargo.toml` lists the main crate as a `[dev-dependencies]` or ordinary `[dependencies]` entry purely to force it to be built first:

```toml
[package]
name = "clank-acceptance-tests"
version = "0.0.0"
publish = false

[dev-dependencies]
clank = { path = "../clank" }         # forces clank binary to be compiled
assert_cmd = "2.1"
predicates = "3.1"
```

### `CARGO_BIN_EXE_<name>` — how it works

`CARGO_BIN_EXE_<name>` is a compile-time environment variable set by Cargo when building *integration tests* (files in `tests/`). It expands to the absolute path of the compiled binary named `<name>`.

**Critical constraint:** this variable is only set when the binary being referenced belongs to a package that is a *direct dependency* (or the package itself) of the test crate. Cargo's documentation states:

> Binary targets are automatically built if there is an integration test or benchmark being selected to test. The `CARGO_BIN_EXE_<name>` environment variable is set when the integration test is built.

Two usage forms:

```rust
// Compile-time (preferred — fails the build if the var is absent):
let path = std::path::PathBuf::from(env!("CARGO_BIN_EXE_clank"));

// Runtime (panics at test time if absent, not build time):
let path = std::env::var("CARGO_BIN_EXE_clank").unwrap();
```

**Same crate vs. separate crate:**

- In the *same* package (e.g., `clank`'s own `tests/`): `CARGO_BIN_EXE_clank` is set automatically when `[[bin]] name = "clank"` exists in that package's `Cargo.toml`. No additional setup needed.
- In a *separate* acceptance crate: the acceptance crate must declare the main crate as a dependency in `Cargo.toml`. This causes Cargo to build it and expose the variable. Without the dependency, Cargo does not build the binary before the tests, and the variable is not set.

`assert_cmd` provides a macro form via `assert_cmd::cargo::cargo_bin!("clank")` which uses `CARGO_BIN_EXE_clank` internally — this is how brush uses it (see section 4).

### `[dev-dependencies]` vs `[dependencies]` for the acceptance crate

In a pure acceptance-test crate that has only `tests/` and no `src/`, both work; `[dev-dependencies]` is conventional and slightly cleaner because it signals "this dependency only matters for tests". However, for forcing a binary build from a separate workspace member, either works.

---

## 3. `assert_cmd` and `predicates`

### `assert_cmd`

- **Current version:** 2.1.2 (released January 2026)
- **MSRV:** 1.74
- **Downloads:** ~51M total, ~3.6M/month; used in 3,091 crates directly
- **License:** MIT OR Apache-2.0
- **Maintained by:** the rust-cli WG (@epage)

`assert_cmd` is the standard library for subprocess-based CLI acceptance testing in Rust. It provides:

1. `Command::cargo_bin(name)` — resolves the compiled binary via `CARGO_BIN_EXE_<name>`, returning a `Command` ready to spawn.
2. `assert_cmd::cargo::cargo_bin!(name)` — macro form, compile-time resolution.
3. Fluent assertion API on `Command`: `.arg()`, `.env()`, `.write_stdin()`, `.assert()` → `.success()` / `.failure()` / `.code(n)` / `.stdout(predicate)` / `.stderr(predicate)`.
4. The `.assert()` result is an `Assert` struct that chains predicate-based checks.

Minimal example:

```rust
use assert_cmd::Command;

#[test]
fn clank_exits_zero_on_empty_script() {
    Command::cargo_bin("clank")
        .unwrap()
        .arg("-c")
        .arg("true")
        .assert()
        .success();
}
```

**Limitation:** `Command::cargo_bin` only works within an integration test context (not from a regular binary). For more flexible binary path resolution (e.g., cross-compilation, arbitrary targets), `escargot` is the lower-level alternative that `assert_cmd` internally uses.

### `predicates`

- **Current version:** 3.1.4 (released February 2026)
- **MSRV:** 1.74
- **Downloads:** ~139M total, ~21M/month; reverse dependencies: 2,360
- **License:** MIT OR Apache-2.0
- **Maintained by:** same rust-cli WG maintainers

`predicates` provides composable boolean-valued predicate functions intended for use in assertions. It is the standard companion to `assert_cmd`.

Key predicates for shell testing:

```rust
use predicates::prelude::*;

// Exact string match
predicate::eq("hello\n")

// Contains substring
predicate::str::contains("error:")

// Regex match
predicate::str::is_match(r"^\d+ items$").unwrap()

// Starts/ends with
predicate::str::starts_with("Usage:")

// Logical combinators
predicate::str::contains("foo").and(predicate::str::contains("bar"))
predicate::str::contains("error").not()
```

Used with `assert_cmd`:

```rust
cmd.assert()
   .stdout(predicate::str::contains("hello"))
   .stderr(predicate::str::is_empty());
```

---

## 4. File-Driven Test Discovery: `datatest-stable`

- **Current version:** 0.3.x (latest as of research date)
- **MSRV:** 1.72
- **Maintained by:** nextest-rs organization (same org as cargo-nextest)
- **License:** MIT OR Apache-2.0

`datatest-stable` provides a stable-Rust file-driven test harness. The user registers a test function and a directory; the harness calls that function once per matching file.

### Setup

**Cargo.toml:**

```toml
[[test]]
name = "shell-cases"
path = "tests/shell_cases.rs"
harness = false           # required — datatest-stable provides its own main()
```

**tests/shell_cases.rs:**

```rust
fn run_case(path: &std::path::Path) -> datatest_stable::Result<()> {
    // read the file, run the shell, assert output
    Ok(())
}

datatest_stable::harness! {
    { test = run_case, root = "tests/cases", pattern = r".*\.sh$" },
}
```

### How it works

- `harness!` expands to a `fn main()` that walks the `root` directory, matches files against `pattern` (a regex, via `fancy-regex` crate), and calls the test function for each match.
- The `Path` passed to the function is `root` joined with the relative file path.
- `pattern` is a regex string expression; it matches against the relative path from `root`.
- Multiple test/root/pattern triples can be listed in one `harness!` invocation.
- Works with `cargo test` (sequential by default) and `cargo nextest` (each file becomes a separate named test, run in parallel as separate processes).

### Test function signatures

```rust
// Path only
fn my_test(path: &Path) -> datatest_stable::Result<()>

// Path + contents
fn my_test(path: &Path, contents: Vec<u8>) -> datatest_stable::Result<()>

// UTF-8 path (via camino)
fn my_test(path: &datatest_stable::Utf8Path) -> datatest_stable::Result<()>
```

### Caveats

- `harness = false` is mandatory. Forgetting it will cause a link error because `main()` is defined twice.
- `CARGO_MANIFEST_DIR` is available inside the test function at runtime to locate fixture files relative to the package root. This is how brush does it: `PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/cases")`.
- The `include-dir` feature allows embedding fixture directories at compile time (for distribution), but is off by default and not recommended for rapidly-changing test data.
- `datatest-stable` is not the same as `datatest` (the nightly-only original). The `-stable` variant works on stable Rust and is actively maintained; the original `datatest` crate is essentially unmaintained.

---

## 5. Brush Oracle Test Strategy

### Architecture overview

Brush uses a custom `brush-test-harness` workspace crate (a library, not published to crates.io) that encapsulates all test execution logic. The main `brush-shell` crate lists it as a `[dev-dependency]`. Test binaries in `brush-shell/tests/` each have `harness = false` and write their own `fn main()` that delegates to `brush-test-harness`.

**Workspace crates involved:**

- `brush-test-harness` — the test framework library
- `brush-shell` — the binary under test; its `Cargo.toml` has both `[lib]` and `[[bin]] name = "brush"`, and multiple `[[test]]` entries

### Test file format

Test cases are **YAML files** in `brush-shell/tests/cases/`. Cases are grouped into two directories:

- `cases/compat/` — oracle-comparison tests (brush vs. bash)
- `cases/brush/` — brush-only expectation/snapshot tests

Each YAML file has a `name` and a list of `cases`. Each case in `TestCase` supports:

| Field | Purpose |
|---|---|
| `name` | Human-readable label |
| `args` | CLI arguments to the shell |
| `stdin` | Script input (the shell command to run) |
| `env` | Environment variables |
| `test_files` | Files to create in a temp directory before the test |
| `removed_default_args` | Default args to strip (e.g., `--norc`) |
| `ignore_stderr` / `ignore_stdout` / `ignore_exit_status` | Suppress specific comparison axes |
| `ignore_whitespace` | Normalize whitespace before comparing |
| `known_failure` | Mark as expected failure (still runs, not a hard fail) |
| `skip` | Skip entirely |
| `incompatible_configs` | Skip for specific oracle configs |
| `incompatible_os` | Skip on specific OS |
| `min_oracle_version` / `max_oracle_version` | Oracle bash version gates |
| `expected_stdout` / `expected_stderr` / `expected_exit_code` | Inline expectations (expectation mode) |
| `snapshot` | Use insta snapshot instead of inline expectation |
| `skip_oracle` | Run test but skip oracle comparison |
| `timeout_in_seconds` | Per-case timeout |

Example from `basic.yaml`:

```yaml
name: "Basic tests"
cases:
  - name: "Basic -c usage"
    args:
      - "-c"
      - "echo hi"

  - name: "Basic script execution"
    test_files:
      - path: "script.sh"
        contents: |
          echo "ARGS: $@"
          exit 22
    args: ["./script.sh", 1, 2, 3]
```

### Runner modes

`brush-test-harness` supports three modes configured in `RunnerConfig`:

1. **Oracle (`TestMode::Oracle`):** runs both the oracle shell (bash) and the test shell, then diffs stdout, stderr, and exit status. Used by `compat_tests.rs`.
2. **Expectation (`TestMode::Expectation`):** runs only the test shell, compares against `expected_stdout`/`expected_stderr`/`expected_exit_code` inline fields or insta snapshots. Used by `integration_tests.rs`.
3. **Hybrid:** both modes simultaneously.

### How the binary path is resolved

Both `compat_tests.rs` and `integration_tests.rs` resolve the binary like this:

```rust
options.brush_path = assert_cmd::cargo::cargo_bin!("brush")
    .to_string_lossy()
    .to_string();
```

This uses `assert_cmd`'s macro which internally reads `CARGO_BIN_EXE_brush`. Since tests live in the same package as the binary (`brush-shell`), the variable is set automatically.

### Execution model

- The test binaries have `harness = false` and write a tokio async `main()`. Parallelism is controlled by the runner (32 worker threads in brush's config).
- The runner walks the YAML directory with `walkdir`, deserializes each file, and runs each `TestCase`. Each case creates a temp directory, writes `test_files` into it, spawns the shell binary as a subprocess, captures stdout/stderr/exit, and compares.
- When running against an oracle, both shells are invoked in the same temp directory setup, and outputs are diffed.

### What can be adapted for clank

The core pattern is directly reusable:

1. **YAML test case format** — the `TestCaseSet` / `TestCase` schema is generic enough to adopt as-is or with minor changes. The key fields for a shell oracle test are: `name`, `stdin`, `args`, `env`, `test_files`, `ignore_stderr`, `ignore_exit_status`, `known_failure`, `skip`.

2. **Oracle strategy** — run the reference shell (bash/sh) and clank on the same input in the same temp directory, diff stdout, stderr, and exit code. This is exactly what `compat_tests.rs` does.

3. **Inline expectations** — for behavior that intentionally diverges from bash (e.g., clank-specific features), use `expected_stdout`/`expected_exit_code` rather than oracle comparison.

4. **Binary path resolution** — `assert_cmd::cargo::cargo_bin!("clank")` works directly, since tests live in the same package.

5. **Custom `TestCase` struct** — if clank uses a simpler YAML schema (e.g., `.sh` files rather than YAML), the runner would change but the subprocess/oracle pattern is identical.

**What not to take directly:**

- `brush-test-harness` is tightly coupled to brush's `WhichShell` enum, insta snapshot integration, PTY support, and `clap`-based `TestOptions` CLI. For clank, writing a simpler purpose-built runner is less work than adapting this framework.
- The tokio async runner is necessary for brush because it has async shell internals. Clank's acceptance tests can use a synchronous runner unless async is needed.

---

## Summary of Crate Versions (as of March 2026)

| Crate | Version | MSRV |
|---|---|---|
| `assert_cmd` | 2.1.2 | 1.74 |
| `predicates` | 3.1.4 | 1.74 |
| `datatest-stable` | 0.3.x | 1.72 |
| `walkdir` | 2.5.x | — |
| `serde_yaml` | 0.9.34 | — |
