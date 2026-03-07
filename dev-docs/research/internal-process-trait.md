---
title: Internal Process Trait — Brush Extension Points and Implementation Strategy
date: 2026-03-07
author: agent
---

## Purpose

Determine how to replace Brush's OS process spawning with clank's internal async
process trait, what extension points Brush exposes, and what the implementation
strategy should be for the first iteration.

---

## Critical Finding: Brush Already Has a WASM Platform Layer

The most important finding from this research: **Brush already has a `sys/wasm.rs`
module** in its `brush-core/src/sys/` directory. It delegates every platform
operation to stub implementations:

```rust
// brush-core/src/sys/wasm.rs
pub use crate::sys::stubs::commands;
pub use crate::sys::stubs::fd;
pub use crate::sys::stubs::fs;
pub use crate::sys::stubs::input;
pub(crate) use crate::sys::stubs::network;
pub(crate) use crate::sys::stubs::pipes;
pub use crate::sys::stubs::process;
pub use crate::sys::stubs::signal;
pub use crate::sys::stubs::terminal;
```

This means `brush-core` **already has conditional compilation** gating `nix` and
OS-specific code behind a `sys` platform abstraction layer. The `wasm.rs` module
routes to stub implementations that return `unimplemented!()` or no-ops. The `nix`
crate is only used inside `sys/unix/` — not in the WASM path.

**Implication:** The `nix` seam is already handled inside Brush. We do not need to
fork `brush-core` or patch it. The WASM compile target issue is a matter of enabling
the right feature/cfg flag when targeting `wasm32-wasip2`, not a fundamental blocker.

This needs to be verified empirically (actually attempt `cargo build --target
wasm32-wasip2`), but the architecture strongly suggests it is already handled.

---

## How Brush Dispatches Commands

Brush's command dispatch (`brush-core/src/commands.rs`) follows this resolution order:

1. Check if the command name matches a registered **builtin** (`Shell::builtins`)
2. If not a builtin, search `$PATH` and spawn a **real OS process** via
   `std::process::Command` / tokio process

The OS process spawning path is in `brush_core::sys::commands` and
`brush_core::sys::process` — both of which are stubbed on WASM.

**The hook point for clank is step 1**: register clank's internal command
implementations as builtins via `brush-core`'s builtin registration API. Because
Brush checks builtins first, any command registered as a builtin is intercepted
before the OS process spawn path is ever reached.

---

## The `brush_core::builtins::Command` Trait

The primary extension point is `brush_core::builtins::Command`:

```rust
pub trait Command: Parser {
    type Error: BuiltinError + 'static;

    fn execute(
        &self,
        context: ExecutionContext<'_>,
    ) -> impl Future<Output = Result<ExecutionResult, Self::Error>> + Send;
}
```

- Derives `clap::Parser` for argument parsing — argument handling is free.
- `execute` is async and receives `ExecutionContext`, which provides:
  - `context.shell` — mutable access to the shell (for parent-shell commands like `cd`)
  - `context.stdout()` — writable stdout (an `OpenFile`-backed `Write`)
  - `context.stderr()` — writable stderr
  - `context.stdin()` — readable stdin
  - `context.params` — `ExecutionParameters` including fd table
- Returns `ExecutionResult` which carries the exit code

This is a complete, clean, async-native hook. Implementing a command as an internal
Rust function requires implementing this trait and registering it via
`Shell::register_builtin("ls", brush_core::builtins::builtin::<LsCommand>())`.

There is also a simpler `SimpleCommand` trait for commands that don't need clap arg
parsing — takes args as a raw iterator.

---

## Strategy: Register All Commands as Builtins

Since Brush checks builtins before falling through to OS process spawn, the strategy
for implementing clank's internal process trait is:

**Register every clank command as a `brush_core::builtins::Command`.**

This gives:
- Full async execution
- stdin/stdout/stderr via `ExecutionContext` — same `OpenFile` pipe mechanism used
  by the golden tests
- Mutable shell access for `parent-shell` scoped commands (`cd`, `export`, etc.)
- Zero OS process spawning
- Works on both native and WASM (stubs handle the WASM platform layer)

The "internal process trait" described in the README is, in practice, the
`brush_core::builtins::Command` trait — Brush has already designed the right
abstraction. clank implements its commands against it.

---

## Minimal First Implementation: `echo`, `true`, `false`

For the first iteration, the plan is to implement three commands:

- `echo` — writes args to stdout; proves the stdout write path works
- `true` — exits 0; proves exit code works
- `false` — exits 1; proves exit code works

These three commands are:
1. Already tested in the integration and golden test suites
2. Simple enough to implement without filesystem or process knowledge
3. Sufficient to prove the architecture is correct

`echo` is particularly important because `brush-builtins` already registers an `echo`
implementation. We will **override** it by registering our own. This proves the
override mechanism works and that clank's builtin takes precedence over Brush's default.

---

## Workspace Structure Change

A new library crate `clank-builtins` will house all clank command implementations.
This keeps them separate from the main `clank` binary crate, makes them independently
testable, and mirrors the `brush-builtins` crate design.

```
clank.sh/
├── clank/               ← binary + lib (REPL, shell construction)
├── clank-builtins/      ← NEW: internal command implementations
│   └── src/
│       ├── lib.rs       ← registers all commands
│       ├── echo.rs
│       ├── true_cmd.rs
│       └── false_cmd.rs
└── clank-golden/        ← golden test runner
```

`clank-builtins` depends on `brush-core` only. `clank` depends on `clank-builtins`
and calls its registration function during shell construction.

---

## WASM Compilation Risk Assessment

Based on Brush's `sys/wasm.rs` stub architecture, the risk is **lower than previously
assessed**. The `nix` crate is only used in `sys/unix/` paths. The WASM path uses
stubs. This means:

- `brush-core` likely already compiles to `wasm32-wasip2` if the correct cfg is active
- The remaining risk is other dependencies in the `clank` crate that may not be
  WASM-compatible (e.g. `tokio` with `features = ["full"]` uses OS threads)
- For `tokio` on WASM, the `wasm32` target requires `tokio` with the appropriate
  feature set — this is a known solvable problem

This will be validated empirically during or after this task.

---

## Conclusions

1. **The hook point is `brush_core::builtins::Command`** — register commands as
   builtins, they are dispatched before OS process spawn.
2. **Brush already handles the WASM platform seam** — `sys/wasm.rs` stubs all
   Unix-specific code. No fork required.
3. **The implementation strategy is: implement commands as `Command` trait impls,
   register them via `Shell::register_builtin`**.
4. **A new `clank-builtins` crate** is the right home for command implementations.
5. **Start with `echo`, `true`, `false`** — minimal, fully testable, proves the
   architecture.
6. **WASM compilation is lower risk than assumed** — should be attempted alongside
   or shortly after this task.
