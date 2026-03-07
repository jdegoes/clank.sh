---
title: Implement Internal Process Trait — First Commands via Brush Builtin Registration
date: 2026-03-07
author: agent
issue: dev-docs/issues/open/internal-process-trait.md
research:
  - dev-docs/research/internal-process-trait.md
designs: []
---

## Summary

Establish clank's internal process dispatch layer by registering command
implementations as `brush_core::builtins::Command` instances. Brush checks
registered builtins before falling through to OS process spawn — registering a
command as a builtin is the hook that intercepts it. A new `clank-builtins` crate
houses all command implementations. The first iteration implements `echo`, `true`,
and `false` to prove the architecture end-to-end.

## Developer Feedback

- Use `brush_core::builtins::Command` as the internal process trait — Brush already
  designed the right abstraction, no separate clank trait is needed.

## Design Decisions and Rationale

### `brush_core::builtins::Command` is the internal process trait

The README states clank replaces Brush's "Unix process spawning and runtime model,
substituted by the internal async process trait." That trait is
`brush_core::builtins::Command`. Brush dispatches to registered builtins before
searching `$PATH`, so a registered builtin is never a real OS process. The trait
provides async execution, stdin/stdout/stderr, mutable shell access, and exit codes —
everything needed. No new abstraction is required.

### New `clank-builtins` crate

Command implementations live in a dedicated library crate `clank-builtins`. This
mirrors the `brush-builtins` crate design, keeps commands independently testable,
and gives a clear home for every future command. `clank-builtins` depends on
`brush-core` only. `clank` depends on `clank-builtins` and calls its registration
function in `build_shell()`.

### Start with `echo`, `true`, `false`

These three commands are already covered by existing integration tests and golden
tests. Implementing them internally proves:
- The stdout write path works (`echo`)
- Exit code 0 works (`true`)
- Exit code 1 works (`false`)
- The override mechanism works (`echo` overrides `brush-builtins`' default)
- No OS process is spawned for any of them

No new test logic is needed — existing tests become the acceptance criteria.

### `clap::Parser` derive for argument parsing

`brush_core::builtins::Command` requires the implementor to also implement
`clap::Parser`. This gives argument parsing for free. For `echo`, `true`, and
`false` this is minimal — but the pattern is established for all future commands.

## Workspace Structure

```
clank.sh/
├── Cargo.toml               ← add clank-builtins to members
├── clank/
│   └── src/lib.rs           ← build_shell() calls clank_builtins::register()
├── clank-builtins/          ← NEW
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs           ← pub fn register(shell: &mut Shell)
│       ├── echo.rs          ← EchoCommand
│       ├── true_cmd.rs      ← TrueCommand
│       └── false_cmd.rs     ← FalseCommand
└── clank-golden/
```

## `clank-builtins` Public API

```rust
/// Register all clank builtin commands on the shell, overriding any
/// brush-builtins defaults for the same command names.
pub fn register(shell: &mut brush_core::Shell);
```

Each command implements `brush_core::builtins::Command` + `clap::Parser` and is
registered via `shell.register_builtin(name, brush_core::builtins::builtin::<Cmd>())`.

## Acceptance Tests

All existing tests serve as acceptance criteria — no new test logic is required:

1. `cargo test` passes with zero failures (all 44 existing tests still green).
2. `cargo test --test golden` passes — `echo-hello.yaml` and `cd-pwd.yaml` golden
   tests use clank's internal `echo`, not a spawned process.
3. Unit tests in `clank-builtins` directly call `register()` and verify commands
   are registered on the shell.
4. `cargo clippy --all-targets -- -D warnings` passes.

## Tasks

- [ ] Add `clank-builtins` to workspace `Cargo.toml` members
- [ ] Create `clank-builtins/Cargo.toml` with dependency on `brush-core` and `clap`
- [ ] Implement `EchoCommand` in `clank-builtins/src/echo.rs` — writes args joined
      by spaces to stdout, appends newline, returns exit code 0
- [ ] Implement `TrueCommand` in `clank-builtins/src/true_cmd.rs` — returns exit
      code 0 immediately
- [ ] Implement `FalseCommand` in `clank-builtins/src/false_cmd.rs` — returns exit
      code 1 immediately
- [ ] Implement `pub fn register(shell: &mut Shell)` in `clank-builtins/src/lib.rs`
      registering all three commands
- [ ] Update `clank/src/lib.rs` `build_shell()` to call `clank_builtins::register()`
      after shell construction
- [ ] Add unit tests in `clank-builtins/src/lib.rs` verifying registration succeeds
- [ ] Verify all acceptance tests pass: `cargo test` and `cargo clippy`
