---
title: "Multi-Tier Test Infrastructure"
date: 2026-03-06
author: agent
issue: "dev-docs/issues/open/multi-tier-test-infrastructure.md"
research:
  - "dev-docs/research/rust-multi-tier-testing.md"
---

# Multi-Tier Test Infrastructure

## Originating Issue

No test infrastructure beyond basic unit tests — see `dev-docs/issues/open/multi-tier-test-infrastructure.md`.

## Research Consulted

- `dev-docs/research/rust-multi-tier-testing.md` — Cargo integration test mechanics, acceptance test crate patterns, `assert_cmd`, `predicates`, `datatest-stable`, and Brush's oracle test strategy.

## Designs Referenced

- `dev-docs/designs/approved/initial-workspace.md` — the existing workspace structure this plan builds on.

## Developer Feedback

Consulted on four design decisions:

- **Test case format:** YAML (adapted from Brush's schema, expectations-only). Human-readable shell scripts with comment annotations were considered but YAML was preferred for its ability to express richer case metadata (name, args, env, known_failure, skip) and multi-case files.
- **Oracle mode:** Deferred. Expectations-only for this issue. Oracle/bash-diff mode is a separate future issue.
- **Library crate:** Rather than splitting `clank-shell` into `src/lib.rs` + `src/main.rs`, introduce a dedicated `clank-core` crate containing all shell logic. `clank-shell` becomes a thin binary wrapper depending on `clank-core`. This is cleaner than a `[lib]` + `[[bin]]` split in the same crate, and makes `clank-core` a proper reusable library for future embedders and test crates.
- **Integration test placement:** Integration tests live in `clank-core/tests/` and call the `clank-core` public API directly.

## Approach

Introduce three test tiers as new workspace structure, without removing any existing tests:

### Tier 1: Unit tests (already in place)
Inline `#[cfg(test)]` modules in each crate. Existing tests in `clank-shell/src/main.rs` move to `clank-core/src/lib.rs` as part of the refactor.

### Tier 2: Integration tests (`clank-core/tests/`)
`clank-core` is a library crate. Integration tests in `clank-core/tests/` call the public API directly — no subprocess. Scope: `Shell::new()` option mapping, `run()` function behaviour.

### Tier 3: Acceptance tests (`clank-acceptance/`)
A new workspace member crate. Uses `datatest-stable` + `assert_cmd` + `predicates` to discover YAML test case files, spawn the compiled `clank` binary as a subprocess, and assert on stdout, stderr, and exit code.

## Workspace Structure (after this plan)

```
Cargo.toml                          # workspace — add clank-core, clank-acceptance
clank-core/
  Cargo.toml                        # NEW — library crate
  src/
    lib.rs                          # NEW — shell logic (run fn, re-exports)
  tests/
    shell_builder.rs                # NEW — integration tests
clank-shell/
  Cargo.toml                        # simplified — depends on clank-core
  src/
    main.rs                         # thin wrapper calling clank_core::run
clank-http/
  ...                               # unchanged
clank-acceptance/
  Cargo.toml                        # NEW — acceptance test crate
  tests/
    acceptance.rs                   # NEW — datatest-stable harness
  cases/
    scripting/
      pipelines.yaml                # NEW
    exit_codes/
      basic.yaml                    # NEW
    builtins/
      alias.yaml                    # NEW
```

## YAML Test Case Schema

Each `.yaml` file under `clank-acceptance/cases/` contains a `name` (describing the suite) and a `cases` list. Each case has:

| Field | Required | Type | Description |
|---|---|---|---|
| `name` | yes | string | Short description; becomes the test name |
| `stdin` | yes | string | Shell script source passed to clank via stdin |
| `args` | no | list of string | Extra argv passed to clank (default: none) |
| `env` | no | map | Extra environment variables |
| `expect_exit` | no | integer | Expected exit code (default: 0) |
| `expect_stdout` | no | string | Expected exact stdout (default: not checked) |
| `expect_stdout_contains` | no | string | Substring that stdout must contain |
| `expect_stderr_empty` | no | bool | If true, assert stderr is empty |
| `known_failure` | no | bool | If true, test is marked ignored at runtime |
| `skip` | no | bool | If true, test is skipped entirely |

Example:

```yaml
name: "Pipeline tests"
cases:
  - name: pipeline_basic
    stdin: |
      echo "hello world"
    expect_exit: 0
    expect_stdout: "hello world\n"
    expect_stderr_empty: true
```

## `clank-core` public API

```rust
pub use brush_core::{CreateOptions, Shell};

/// Boot a shell and run a single command string, returning the numeric exit code.
pub async fn run(command: &str) -> Result<u8, brush_core::Error>
```

`run` constructs a shell with non-interactive options (no profile, no rc, no editing) and executes the command via `Shell::run_string`. This is the same logic currently in `clank-shell/src/main.rs`, moved to the library.

## `clank-shell/src/main.rs` (after refactor)

```rust
#[tokio::main]
async fn main() -> ExitCode {
    // read from argv or stdin
    match clank_core::run(&command).await { ... }
}
```

`clank-shell` depends only on `clank-core` and `tokio`. All brush-core imports are removed from `clank-shell`.

## `clank` binary stdin mode

The acceptance tier passes shell scripts via stdin. The binary must support reading commands from stdin when no argv is given. Currently `main.rs` defaults to `"echo hello"` — this will change: with no args, read the full contents of stdin and execute that as the command string.

## `clank-acceptance/Cargo.toml`

```toml
[dev-dependencies]
clank-shell     = { path = "../clank-shell" }   # forces binary build; exposes CARGO_BIN_EXE_clank
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

## Initial test cases (demonstrating all three tiers)

### Tier 1 (unit) — `clank-core/src/lib.rs`
- `echo_hello_exits_zero` (moved from clank-shell)
- `false_exits_nonzero` (moved from clank-shell)

### Tier 2 (integration) — `clank-core/tests/shell_builder.rs`
- `shell_new_default_options_succeeds` — `Shell::new(CreateOptions::default())` completes without error
- `run_echo_returns_zero` — `clank_core::run("echo hello")` returns exit code 0
- `run_false_returns_nonzero` — `clank_core::run("false")` returns non-zero

### Tier 3 (acceptance) — YAML case files
- `cases/scripting/pipelines.yaml` — basic `echo`, pipeline basics
- `cases/exit_codes/basic.yaml` — `true`, `false`, explicit `exit 42`
- `cases/builtins/alias.yaml` — define and invoke an alias

## Acceptance Tests

- `cargo test --workspace` passes all tiers
- `cargo test -p clank-core` passes unit + integration tests
- `cargo test -p clank-acceptance` passes acceptance tests (requires compiled `clank` binary)
- `cargo nextest run --workspace` passes (datatest-stable is nextest-compatible)
- Adding a new `.yaml` file under `cases/` is automatically picked up without any code changes

## Tasks

- [ ] Create `clank-core` library crate: `Cargo.toml` with brush-core, brush-parser, brush-builtins, tokio deps; `src/lib.rs` with `run()` function and public re-exports; move existing unit tests from `clank-shell/src/main.rs` into `clank-core/src/lib.rs`
- [ ] Update `clank-shell`: remove brush-core deps, add `clank-core` dependency; simplify `src/main.rs` to call `clank_core::run`
- [ ] Add stdin reading to `clank` binary: when no args are provided, read full stdin as the command string
- [ ] Add `clank-core` and `clank-acceptance` to root `Cargo.toml` workspace members
- [ ] Add `clank-core/tests/shell_builder.rs` with integration tests for `Shell::new` and `clank_core::run`
- [ ] Create `clank-acceptance` crate: `Cargo.toml` with `datatest-stable`, `assert_cmd`, `predicates`, `serde`, `serde_yaml`, `clank-shell` dev-dep
- [ ] Implement `clank-acceptance/tests/acceptance.rs`: YAML deserialisation, binary spawning via `assert_cmd`, assertion logic, `datatest_stable::harness!` wiring
- [ ] Add `cases/scripting/pipelines.yaml` with at least 2 cases
- [ ] Add `cases/exit_codes/basic.yaml` with at least 3 cases (`true`, `false`, `exit 42`)
- [ ] Add `cases/builtins/alias.yaml` with at least 1 case
- [ ] Verify `cargo test --workspace` passes
- [ ] Verify `cargo clippy --workspace -- -D warnings` passes
- [ ] Verify `cargo fmt --all --check` passes
