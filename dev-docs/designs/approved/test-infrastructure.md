---
title: Test Infrastructure — Realized Design
date: 2026-03-07
author: agent
---

## Overview

This document records the test infrastructure as actually built. It supersedes any prior approved design for this area (none existed).

## Final Structure

```
clank/
├── src/
│   ├── lib.rs          ← library crate: all shell logic + unit tests
│   └── main.rs         ← thin binary entry point (4 lines)
└── tests/
    ├── common/
    │   └── mod.rs      ← shared helpers: clank(), run_script()
    ├── repl.rs         ← integration tests: REPL loop behaviour
    ├── exit_codes.rs   ← integration tests: $? propagation
    ├── builtins.rs     ← integration tests: echo, pwd, cd, ls, true, false
    └── system.rs       ← system tests: multi-step realistic scenarios
```

## Library/Binary Split

`clank/src/main.rs` is now a 4-line thin wrapper:

```rust
#[tokio::main]
async fn main() {
    let shell = clank::build_shell().await;
    clank::run_repl(shell).await;
}
```

All logic lives in `clank/src/lib.rs`, which exposes two public async functions:

- `pub async fn build_shell() -> Shell` — constructs a `brush_core::Shell` with `BashMode` builtins, `no_profile`, `no_rc`, and `shell_name = "clank"`.
- `pub async fn run_repl(mut shell: Shell)` — reads stdin line by line, dispatches to `shell.run_string()`, writes prompt to stderr.

## Unit Tests (`src/lib.rs` — `#[cfg(test)]`)

Five unit tests live directly in `lib.rs` under `mod tests`:

| Test | What it verifies |
|---|---|
| `build_shell_succeeds` | Shell construction does not panic |
| `shell_runs_true` | `run_string("true")` returns `Ok` |
| `shell_exit_code_zero_after_success` | `shell.last_result()` is `0` after `true` |
| `shell_exit_code_nonzero_after_failure` | `shell.last_result()` is non-zero after `false` |
| `shell_name_is_clank` | `echo $0` executes without error |

Key API note: exit status is read via `shell.last_result() -> u8` (not `last_exit_status` — that field is private). `#[tokio::test]` is used for all async unit tests.

## Shared Test Helpers (`tests/common/mod.rs`)

```rust
pub fn clank() -> Command          // Command pointing at the clank binary
pub fn run_script(script: &str) -> Assert  // Write script to stdin, return Assert
```

`run_script` automatically appends a trailing newline if missing. All integration and system test files declare `mod common;` to access these.

## Integration Tests

### `tests/repl.rs` (8 tests)
REPL loop behaviour: basic echo, multiple commands, empty line skipping, whitespace-only line skipping, prompt on stderr not stdout, `exit` command, EOF, commands-after-exit not running.

### `tests/exit_codes.rs` (7 tests)
`$?` propagation: zero after success, one after `false`, zero after echo, updates each command, non-zero after failure, variable assignment exits zero, clank process itself always exits zero.

### `tests/builtins.rs` (13 tests)
Per-builtin correctness: `echo` (single word, multiple words, empty, stdout not stderr), `true`/`false` (exit codes), `pwd` (outputs a path, exits zero), `cd` (valid dir exits zero, updates working dir, invalid dir exits non-zero), `ls` (produces output, non-existent path exits non-zero).

## System Tests (`tests/system.rs` — 8 tests)

Multi-step realistic scenarios exercising shared shell state:

| Test | Scenario |
|---|---|
| `scenario_variable_assign_and_expand` | `name=world; echo hello $name` → `hello world` |
| `scenario_variable_persists_across_commands` | Set then echo a variable |
| `scenario_variable_reassignment` | Reassign and verify new value |
| `scenario_exit_code_tracks_last_command` | `true; false; echo $?` → `1` |
| `scenario_recover_after_failure` | `false; true; echo $?` → `0` |
| `scenario_simple_pipe` | `echo piped_value \| cat` passes value through |
| `scenario_cd_then_pwd` | `cd /tmp; pwd` contains `tmp` |
| `scenario_cd_and_list` | `cd /tmp; ls` succeeds |

## Test Count Summary

| Layer | Location | Count |
|---|---|---|
| Unit | `src/lib.rs` | 5 |
| Integration — REPL | `tests/repl.rs` | 8 |
| Integration — Exit codes | `tests/exit_codes.rs` | 7 |
| Integration — Builtins | `tests/builtins.rs` | 13 |
| System | `tests/system.rs` | 8 |
| **Total** | | **41** |

All 41 tests pass with `cargo test`.

## Deviations from the Approved Plan

- The `tests/system/basic_scripts.rs` subdirectory layout was abandoned. Cargo only auto-discovers top-level `tests/*.rs` files. System tests live in `tests/system.rs` directly, which is simpler and equally clear.
- The plan mentioned deleting the old `clank/tests/repl.rs`. The file was replaced in-place (rewritten with the new content) rather than deleted and recreated — same outcome, cleaner git history.
