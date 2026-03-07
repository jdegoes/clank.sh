---
title: Golden Tests — Realized Design
date: 2026-03-07
author: agent
---

## Overview

This document records the golden test infrastructure as actually built. It supersedes
any prior approved design for this area (none existed).

---

## What Was Built

A YAML-driven golden test framework consisting of:

- A `clank-golden` helper crate containing all shared runner logic.
- A `build.rs` in the `clank` crate that generates one `#[test]` per discovered YAML
  file at compile time.
- A set of YAML fixture files covering builtins, variables, and reusable setup scripts.

No Rust is required to add a new golden test. Adding a YAML file and running
`cargo build && cargo test` is sufficient.

---

## Workspace Structure

```
clank.sh/
├── clank/
│   ├── build.rs                              ← discovers YAML, generates test file
│   └── tests/
│       ├── golden/
│       │   ├── setup/                        ← reusable setup scripts (not test cases)
│       │   │   └── assign-name.yaml
│       │   ├── builtins/
│       │   │   ├── echo-hello.yaml
│       │   │   └── cd-pwd.yaml
│       │   └── variables/
│       │       └── expand-after-assign.yaml
│       └── golden_generated.rs               ← @generated; committed; not hand-edited
└── clank-golden/
    ├── Cargo.toml
    └── src/
        └── lib.rs                            ← GoldenTest, runner, setup chain, assertions
```

---

## YAML Schema

```yaml
description: human-readable label (becomes the test function name)
setup: relative/path/to/setup-file.yaml   # optional
script: |
  one or more lines of shell input
stdout: |                                  # optional — omit to skip stdout assertion
  expected exact stdout
stderr: |                                  # optional — omit to skip stderr assertion
  expected exact stderr
exit_code: 0                               # optional — presence makes this a complete test
```

### Partial vs Complete Tests

- **Partial** (`exit_code` absent): runner feeds all input lines, asserts stdout/stderr
  captured so far, then drops the shell instance. No wait for exit.
- **Complete** (`exit_code` present): runner feeds all input lines, drops params (closes
  write ends of pipes), reads captured output, then asserts exit code and stdout/stderr.

---

## `clank-golden` Crate

### Dependencies

| Crate | Role |
|---|---|
| `brush-core` | `Shell` type and `OpenFile` for pipe-based capture |
| `clank` | `build_shell()` for shell construction |
| `serde` + `serde_yaml` | YAML deserialisation into `GoldenTest` |
| `tokio` | Async runtime for `block_on` inside sync `#[test]` functions |

### Public API

```rust
pub struct GoldenTest {
    pub description: String,
    pub setup: Option<String>,
    pub script: String,
    pub stdout: Option<String>,
    pub stderr: Option<String>,
    pub exit_code: Option<i32>,
}

pub fn load(content: &str) -> GoldenTest
pub fn resolve_setup_chain(test: &GoldenTest, base_dir: &Path) -> Vec<String>
pub fn run(test: &GoldenTest, base_dir: &Path)
pub fn run_embedded(yaml_content: &str, base_dir: &str)  // called from generated tests
```

### Setup Chain Resolution

`resolve_setup_chain` recursively loads `setup` references depth-first, returning an
ordered `Vec<String>` of scripts to run before the test script. Cycle detection panics
with a descriptive message. Setup files may themselves reference a `setup`.

### Output Capture

stdout and stderr are captured using `std::io::pipe()` which returns a
`(PipeReader, PipeWriter)` pair. The `PipeWriter` is wrapped in `OpenFile::PipeWriter`
and set on `ExecutionParameters` via `params.set_fd(1, ...)` and `params.set_fd(2, ...)`.

After executing the test script, `params` is dropped to close the write ends of the pipes.
The read ends are then drained to `String` for assertion.

Setup scripts run with the default (non-capturing) `ExecutionParameters` — their output
is not captured and not asserted on. Only the test script's output is captured.

### Assertion

`assert_output_eq` produces a human-readable line-by-line diff on mismatch, showing
which line differed, the expected value, and the actual value. It panics with this
message — no external diff crate required.

### Runtime

Golden tests are synchronous `#[test]` functions. `clank-golden` builds a
`tokio::runtime::Builder::new_current_thread()` runtime internally and calls
`rt.block_on(...)` to drive the async shell execution.

---

## Code Generation (`build.rs`)

`clank/build.rs` runs at compile time:

1. Walks `clank/tests/golden/` recursively.
2. Skips any directory named `setup/`.
3. For each `.yaml` file found, emits a `#[test]` function into
   `clank/tests/golden_generated.rs`.
4. Test names are derived from the relative path: `/` and `-` replaced with `_`,
   prefixed with `golden_`.
5. Paths in generated code use `concat!(env!("CARGO_MANIFEST_DIR"), "/...")` for
   `include_str!` — portable across machines.
6. Emits `cargo:rerun-if-changed=tests/golden` so Cargo re-runs the script when
   any fixture changes.

Example generated test:

```rust
#[test]
fn golden_builtins_echo_hello() {
    clank_golden::run_embedded(
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden/builtins/echo-hello.yaml")),
        &format!("{}/tests/golden/builtins", env!("CARGO_MANIFEST_DIR")),
    );
}
```

`golden_generated.rs` is committed to git. It is regenerated on every `cargo build`.
The `@generated` comment at the top signals it should not be hand-edited.

---

## Cargo Configuration

```toml
# clank/Cargo.toml

[dev-dependencies]
clank-golden = { path = "../clank-golden" }

[[test]]
name = "golden"
path = "tests/golden_generated.rs"
```

The `[[test]]` section tells Cargo to treat `golden_generated.rs` as a named test
target. It uses the standard libtest harness (no `harness = false`).

---

## Running Golden Tests

```sh
cargo test                        # runs all tests including golden
cargo test --test golden          # runs only golden tests
cargo test --test golden echo     # runs golden tests whose name contains "echo"
```

---

## Fixtures

| File | Type | What it tests |
|---|---|---|
| `setup/assign-name.yaml` | Setup only | Sets `name=world`; no assertions |
| `builtins/echo-hello.yaml` | Partial | `echo hello` → stdout `hello\n` |
| `builtins/cd-pwd.yaml` | Partial | `cd /tmp; pwd` → stdout `/tmp\n` |
| `variables/expand-after-assign.yaml` | Partial + setup chain | Uses `assign-name.yaml`, asserts `echo $name` → `world\n` |

---

## Deviations from the Approved Plan

- The plan described a `run_script_no_capture` helper and a `run_script_captured` helper
  as separate internal functions. Both were implemented as described.
- The plan mentioned "complete test: send EOF, wait for shell to exit". In practice,
  the golden runner does not use a REPL loop — it calls `shell.run_string()` directly.
  For complete tests, the exit code is read from `shell.last_result()` after the final
  `run_string` call. There is no EOF to send. The semantics are equivalent: all input
  is processed, then the exit code is checked.
- `golden_generated.rs` is committed to git as planned. Absolute paths were initially
  generated but replaced with `CARGO_MANIFEST_DIR`-relative paths before the final commit
  for portability.
