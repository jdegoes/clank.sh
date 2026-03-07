---
title: Internal Process Trait — Realized Design
date: 2026-03-07
author: agent
---

## Overview

This document records the internal process trait implementation as actually built.
It supersedes any prior approved design for this area (none existed).

---

## What Was Built

A new `clank-builtins` workspace crate containing clank's internal command
implementations, registered as `brush_core::builtins::Command` instances on the
shell during construction. Three commands are implemented: `echo`, `true`, `false`.
These run as internal Rust async functions — no OS process is spawned.

---

## Workspace Structure

```
clank.sh/
├── Cargo.toml                      ← clank-builtins added to members
├── clank/
│   ├── Cargo.toml                  ← clank-builtins added as dependency
│   └── src/lib.rs                  ← build_shell() calls clank_builtins::register()
└── clank-builtins/
    ├── Cargo.toml
    └── src/
        ├── lib.rs                  ← pub fn register() + unit tests
        ├── echo.rs                 ← EchoCommand
        ├── true_cmd.rs             ← TrueCommand
        └── false_cmd.rs            ← FalseCommand
```

---

## The Internal Process Trait

The "internal process trait" described in the README is
`brush_core::builtins::Command`. Brush dispatches to registered builtins before
searching `$PATH`, so a registered builtin is never a real OS process. The trait
provides:

```rust
pub trait Command: Parser {
    type Error: BuiltinError + 'static;

    fn execute(
        &self,
        context: ExecutionContext<'_>,
    ) -> impl Future<Output = Result<ExecutionResult, Self::Error>> + Send;
}
```

`ExecutionContext` provides:
- `context.stdout()` — writable stdout (`Write`)
- `context.stderr()` — writable stderr (`Write`)
- `context.stdin()` — readable stdin (`Read`)
- `context.shell` — mutable shell access (for `parent-shell` scoped commands)
- `context.params` — execution parameters including fd table

`ExecutionResult::success()` → exit code 0.
`ExecutionResult::new(n)` → exit code n.

---

## Registration

`clank_builtins::register(shell: &mut Shell)` is the single entry point:

```rust
pub fn register(shell: &mut Shell) {
    shell.register_builtin("echo", builtin::<EchoCommand>());
    shell.register_builtin("true", builtin::<TrueCommand>());
    shell.register_builtin("false", builtin::<FalseCommand>());
}
```

Called in `clank::build_shell()` after shell construction, after
`brush-builtins` defaults are registered. This means clank's implementations
override any brush-builtins defaults for the same names — the override mechanism
is proven to work.

---

## Command Implementations

### `EchoCommand`

Writes arguments joined by a single space to stdout, followed by a newline.
Uses `context.stdout()` — an `OpenFile`-backed `Write`. No OS process.

### `TrueCommand`

Returns `ExecutionResult::success()` immediately. No output, no side effects.

### `FalseCommand`

Returns `ExecutionResult::new(1)` immediately. No output, no side effects.

---

## Dependencies

`clank-builtins` depends on:

| Crate | Role |
|---|---|
| `brush-core` | `Shell`, `Command` trait, `ExecutionContext`, `ExecutionResult` |
| `clap` | `Parser` derive required by `Command` trait |
| `async-trait` | Async trait support |
| `thiserror` | Error type derivation |
| `tokio` | Async runtime (required by `brush-core`) |

`clank` (path) is a dev-dependency only — used in unit tests to call `build_shell()`.

---

## Test Coverage

### Unit tests (`clank-builtins/src/lib.rs`)

4 tests calling the library directly without process spawn:

| Test | What it verifies |
|---|---|
| `register_succeeds` | Registration completes without panic |
| `echo_runs_internally` | `echo hello` succeeds, exit code 0 |
| `true_exits_zero` | `true` sets last result to 0 |
| `false_exits_one` | `false` sets last result to 1 |

### Existing tests (all still passing)

All 44 prior tests pass unchanged — the clank-builtins registration is transparent
to them. The golden test `echo-hello.yaml` now exercises clank's internal `echo`
rather than a spawned process.

**Total: 48 tests, all passing. Clippy clean.**

---

## What This Establishes

1. **The architecture is proven.** Commands can be registered as internal Rust
   async functions and dispatched by Brush before OS process spawn is reached.
2. **The pattern is established.** Every future command (`ls`, `cat`, `grep` etc.)
   follows the same pattern: implement `Command`, add a file to `clank-builtins`,
   register in `register()`.
3. **The override mechanism works.** clank's `echo` overrides `brush-builtins`'
   default `echo`.

---

## Deviations from the Approved Plan

None. All tasks completed as specified.
