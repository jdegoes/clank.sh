---
title: Golden Test Infrastructure — Tooling and Approach Research
date: 2026-03-07
author: agent
---

## Purpose

Determine the right tooling and structural approach for a YAML-driven golden test runner
that integrates with `cargo test`, supports setup chaining, partial and complete test
modes, and requires no Rust to add new tests.

---

## File-Driven Test Runner Options

### `datatest-stable` (nextest-rs)

- Walks a directory, discovers files matching a pattern, runs each as a named test.
- Uses `libtest-mimic` under the hood.
- Requires `harness = false`. Stable Rust only.
- Does not natively support YAML parsing — that logic lives in the runner anyway.
- Downside: opinionated about test collection. Less control over the test name and
  lifecycle than writing against `libtest-mimic` directly.

### `libtest-mimic`

- Lets you collect `Trial` objects from any source and run them as `cargo test` tests.
- Full control over what constitutes a test case, test naming, and execution.
- `harness = false`, stable Rust. Used by `datatest-stable` itself.

### `include_str!` + standard `#[test]`

- Use a build script or a macro to generate one `#[test]` function per YAML file at
  compile time using `include_str!`.
- No `harness = false`, no extra dependencies. Works with the standard test harness.
- Each YAML file's path is baked into the binary at compile time — test names are
  derived from the file path.
- Downside: adding a new YAML file requires a rebuild (expected anyway). The build
  script or macro must enumerate files at compile time.
- This is the lowest-dependency option and keeps the runner in the standard harness.

### Hand-rolled with `walkdir` + `libtest-mimic`

- Walk `tests/golden/` at runtime, parse YAML, register as `libtest-mimic` trials.
- Zero magic. Easiest to customise.
- Requires `walkdir`, `serde_yaml`, `libtest-mimic` as dev-dependencies.

## Conclusion on Tooling

The `include_str!` approach combined with a build script is the lowest-dependency
option and avoids `harness = false` complexity. However, it requires a `build.rs`
and code generation, which adds its own complexity.

The simplest correct approach overall: use the standard `#[test]` harness with a
build script that generates one `#[test]` function per YAML file, embedding the YAML
content with `include_str!`. This avoids `libtest-mimic`, `walkdir`, and `harness = false`
entirely.

For YAML parsing, `serde_yaml` is the natural choice. It is already in the ecosystem
and has no native alternatives that are simpler. This is the one dependency worth
taking.

**Selected approach:** build script (`build.rs`) generates `tests/golden_generated.rs`
with one `#[test]` per discovered YAML file. Each test uses `include_str!` to embed
the file content and `serde_yaml` to parse it at test runtime. A `clank-golden` helper
crate contains the shared runner logic (YAML struct, setup chain resolution, shell
execution, assertion).

---

## Partial vs Complete Tests

Two test modes, distinguished by the presence of `exit_code` in the YAML:

**Partial test** (no `exit_code`):
- Runner feeds all input lines to the shell.
- Asserts on stdout/stderr captured so far.
- Shell process/instance is terminated after assertions pass.
- Does not wait for shell to exit naturally.

**Complete test** (`exit_code` present):
- Runner feeds all input lines, sends EOF.
- Waits for the shell to exit.
- Asserts exit code, then stdout/stderr.

---

## YAML Schema

```yaml
description: human-readable label (used as the test name in cargo test output)
setup: relative/path/to/setup-file.yaml   # optional — run in the same shell before script
script: |
  one or more lines of shell input
stdout: |                                  # optional — omit to skip stdout assertion
  expected exact stdout
stderr: |                                  # optional — omit to skip stderr assertion
  expected exact stderr
exit_code: 0                               # optional — if present, makes this a complete test
```

- `stdout` and `stderr` are both optional independently.
- `exit_code` absent → partial test. `exit_code` present → complete test.
- `setup` is a relative path to another `.yaml` file whose `script` is run first
  in the **same shell instance**.
- Setup files may themselves have a `setup`, enabling chains.
- Setup files have no assertions — they exist only to establish shell state.

---

## Setup Chain Model

All scripts in a chain (setup and test) run in the **same `brush_core::Shell` instance**.
This is the only model that gives the test script access to variables, working directory
changes, and other state established by setup scripts.

Snapshotting or cloning shell state is not feasible with the current `brush_core::Shell`
API and would be unnecessarily complex.

---

## `clank-golden` Helper Crate

A separate workspace crate `clank-golden` contains:
- The `GoldenTest` struct (deserialised from YAML).
- The setup chain resolver.
- The shell runner (instantiates `clank::build_shell()`, runs scripts, captures output).
- The assertion logic with clear diffs on mismatch.

This keeps the generated test file thin and the shared logic testable and reusable.

---

## Directory Layout

```
clank.sh/
├── clank/
│   └── tests/
│       ├── golden/
│       │   ├── setup/              ← reusable setup scripts; excluded from discovery
│       │   │   └── assign-name.yaml
│       │   ├── builtins/
│       │   │   ├── echo-hello.yaml
│       │   │   └── cd-pwd.yaml
│       │   └── variables/
│       │       └── expand-after-assign.yaml
│       └── golden_generated.rs     ← generated by build.rs; not hand-edited
└── clank-golden/
    └── src/
        └── lib.rs                  ← GoldenTest, runner, setup chain, assertions
```

---

## Integration with `cargo test`

Because the generated file uses standard `#[test]`, no `harness = false` is needed.
`cargo test` discovers the golden tests automatically alongside unit and integration tests.

Run only golden tests: `cargo test golden`
Run a specific test: `cargo test golden::builtins::echo_hello`
