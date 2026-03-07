---
title: Establish Formal Test Infrastructure (Unit, Integration, System)
date: 2026-03-07
author: agent
issue: dev-docs/issues/open/test-infrastructure.md
research:
  - dev-docs/research/test-infrastructure.md
designs: []
---

## Summary

Establish a well-structured, layered test infrastructure across unit, integration, and system test levels. This includes splitting `main.rs` into a library crate + thin binary (enabling deep unit tests), reorganising the existing integration tests by concern, introducing shared test helpers, and adding a system test layer with multi-step scenario tests. All existing tests are migrated and continue to pass.

## Developer Feedback

- **lib/binary split**: Developer confirmed the split should be done as part of this task. `clank/src/lib.rs` will contain all testable logic; `main.rs` will be a thin entry point that calls into it.

## Design Decisions and Rationale

### Library/binary split

Without `lib.rs`, internal logic (shell construction, REPL loop, command dispatch) can only be tested by spawning the binary. This is slow and cannot test private/internal behaviour. Splitting into `lib.rs` + `main.rs` is the standard Rust solution and is the prerequisite for meaningful unit tests.

`main.rs` becomes:
```rust
#[tokio::main]
async fn main() {
    clank::run().await;
}
```

All logic moves to `lib.rs` (or sub-modules under `src/`).

### Three test layers

| Layer | Location | What it tests | How |
|---|---|---|---|
| Unit | `src/**/#[cfg(test)]` | Internal functions, private logic, shell construction | Direct function calls, `#[tokio::test]` for async |
| Integration | `clank/tests/` (files by concern) | Public API, shell behaviours end-to-end | `assert_cmd` spawning the binary, or direct lib calls |
| System | `clank/tests/system/` | Realistic multi-step scenarios | `assert_cmd` with multi-line stdin scripts |

### Shared helpers in `tests/common/mod.rs`

Common operations extracted into a shared module:
- `fn clank() -> Command` — builds an `assert_cmd::Command` for the binary
- `fn run_script(script: &str) -> Assert` — runs a multi-line script and returns assertions
- Assertion helpers for stdout, stderr, exit code

### Integration test files organised by concern

Rather than one flat `repl.rs`, tests are split by what they cover:

```
clank/tests/
├── common/
│   └── mod.rs          ← shared helpers
├── repl.rs             ← REPL loop behaviour (prompt, EOF, empty lines, exit)
├── builtins.rs         ← builtin command correctness (echo, cd, pwd, etc.)
├── exit_codes.rs       ← exit code propagation ($?, true/false, pipelines)
└── system/
    └── basic_scripts.rs  ← multi-step realistic scenarios
```

### `#[tokio::test]` for async unit tests

Shell construction and `run_string` calls are async. The `#[tokio::test]` attribute (already available via the `tokio` dependency) is used for async unit tests — no new crate needed.

### No new test crates for now

The existing `assert_cmd` and `predicates` dev-dependencies are sufficient. `tempfile` and `assert_fs` will be added when filesystem-touching tests appear in future tasks.

## Acceptance Tests

1. `cargo test` passes with zero failures.
2. `cargo test --lib` runs unit tests in `src/` and they pass.
3. `cargo test --test repl` runs REPL integration tests and they pass.
4. `cargo test --test exit_codes` runs exit code integration tests and they pass.
5. `cargo test --test builtins` runs builtin integration tests and they pass.
6. `cargo test --test basic_scripts` runs system tests and they pass.
7. A new developer can read `tests/common/mod.rs` and add a test in under 5 minutes using the shared helpers.
8. Unit tests directly call `clank::build_shell()` without spawning a process.

## Tasks

- [ ] Split `clank/src/main.rs` into `clank/src/lib.rs` + `clank/src/main.rs`; expose `pub async fn build_shell() -> Shell` and `pub async fn run_repl(shell: Shell)` from `lib.rs`
- [ ] Verify `cargo build` and existing `cargo test` still pass after the split
- [ ] Create `clank/tests/common/mod.rs` with `clank()` helper and `run_script()` helper
- [ ] Create `clank/tests/repl.rs` — migrate and expand existing REPL tests (EOF, empty line, exit, prompt on stderr)
- [ ] Create `clank/tests/exit_codes.rs` — migrate and expand exit code tests (`$?` after success, failure, pipelines)
- [ ] Create `clank/tests/builtins.rs` — add integration tests for `echo`, `pwd`, `cd`, `true`, `false`
- [ ] Create `clank/tests/system/basic_scripts.rs` — add system tests for multi-step scenarios (variable assignment + expansion, command sequence, pipe)
- [ ] Add unit tests in `clank/src/lib.rs` under `#[cfg(test)]` covering `build_shell()` construction and `run_repl` with a trivial command
- [ ] Delete `clank/tests/repl.rs` (the old flat file, replaced by the reorganised structure)
- [ ] Verify all acceptance tests pass
