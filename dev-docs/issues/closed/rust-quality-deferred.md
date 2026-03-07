---
title: "Rust quality: deferred items from code quality audit"
date: 2026-03-06
author: agent
---

# Rust quality: deferred items from code quality audit

## Problem

The code quality audit identified several issues that were initially excluded from the
primary remediation plan either because they were assessed as "safe as written" or because
they required a larger scope of change. On further reflection, each of these has a clear
correct solution, a real payoff, and is actionable without architectural heroics.

## Issues

### 1. `std::sync::Mutex` in `MockHttpClient` is fragile under async use

`MockHttpClient::send` holds `std::sync::Mutex` guards for request recording. While the
critical sections currently do not span `.await` points, `std::sync::Mutex` held in an
`async fn` will deadlock if an `.await` is ever introduced while the guard is live — and
the compiler will not catch it. `tokio::sync::Mutex` would fail to compile if misused in
this way, making the constraint explicit and enforced.

### 2. Pervasive `.unwrap()` on lock acquisitions in production code violates AGENTS.md

AGENTS.md states: "No `unwrap()` or `expect()` outside tests." There are 84 `.unwrap()`
calls in non-test `clank-shell` production code, the majority on RwLock/Mutex acquisitions.
While lock poisoning in a single-process shell REPL is unlikely in practice, `.expect()`
with a diagnostic message is the correct form: it is explicit about the recovery assumption,
produces a better panic message, and costs nothing.

### 3. `Result<_, String>` as error type in `ask_process.rs` violates AGENTS.md

AGENTS.md states: "Error types are typed enums with distinct variants — never stringly
typed." `AskFlags::parse` returns `Result<_, String>` and `select_provider` returns
`Result<_, String>`. These should be typed `ParseError` and `ProviderError` respectively.
Callers currently format the `String` directly into stderr, which works, but means no
caller can ever pattern-match on the error kind.

### 4. `context_process.rs` duplicates config loading that already exists in `clank-ask`

`load_summarize_config()` in `context_process.rs` re-implements config file path
resolution, TOML parsing, and API key extraction — all of which are already implemented
correctly in `clank-ask::config`. The duplicate config path has drifted (different
`unwrap_or_default()` vs `unwrap_or_else` handling of missing home dir). Adding
`clank-ask` as a dependency of `clank-shell` is non-circular (neither depends on the
other; both depend on `clank-http`) and eliminates the duplication.

### 5. Global dispatch table leaks entries when `ClankShell` is dropped

`DISPATCH` and `TRANSCRIPTS` are static `HashMap`s keyed by `shell_id`. Entries are
inserted at `ClankShell::new()` but never removed. In a test suite that creates 50+
`ClankShell` instances, all registered commands and transcripts remain in the static maps
indefinitely. `ClankShell` should implement `Drop` to deregister its entries.

### 6. `run_interactive` performs blocking I/O on the Tokio executor thread

`shell.rs:run_interactive` calls `stdin.lock().read_line()` — a blocking syscall — inside
an `async fn`. In Tokio's single-threaded runtime, this blocks all async work on the
executor thread for the duration of each line read. The correct pattern is
`tokio::task::spawn_blocking(|| stdin.lock().read_line(...))` or using
`tokio::io::AsyncBufReadExt`.

### 7. `Vfs` trait is read-only; write commands bypass the abstraction entirely

`mkdir`, `touch`, and `rm` call `std::fs` directly, bypassing the `Vfs` abstraction. This
means they cannot be tested with `MockVfs`, cannot be portably targeted to WASM, and
cannot be virtualized for future features (e.g. sandboxed execution, MCP resource mounts).
`Vfs` should expose `write_file`, `create_dir`, `create_dir_all`, `remove_file`, and
`remove_dir_all`. `MockVfs` should implement them in-memory. The three commands should
be migrated to use `self.vfs`.

## Out of scope

- Architectural restructuring (extracting `clank-process` crate, injecting
  `ManifestRegistry` as a dependency rather than a singleton) — these are correct
  long-term goals but require careful phasing and are separate issues.
- `run_line` blocking filesystem operations — lower priority than the stdin issue and
  already partially mitigated by the temp-file capture design.
