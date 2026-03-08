---
title: "Transcript Data Structure and context Builtin"
date: 2026-03-07
author: agent
---

# Transcript Data Structure and context Builtin

## Overview

The shell maintains a sliding-window transcript of everything executed in the
current session. The transcript is a first-class shell-owned value: it is not
a terminal side-effect or a log file, but a structured type operated on
directly by the `context` builtin. This design documents the realized
implementation.

## Crate Structure

A new workspace crate, `clank-transcript`, owns the `Transcript` type and the
process-global accessor. Separating these into their own crate is not a
convenience — it is required by the dependency graph:

```
clank-shell → clank-core ──────────────────────────────────→ clank-builtins
                    │                                               │
                    └──────────→ clank-transcript ←────────────────┘
                                       (stdlib only)
```

`clank-core` already depends on `clank-builtins`. If the transcript type lived
in `clank-core`, `clank-builtins` could not depend on it without creating a
cycle. `clank-transcript` has no dependencies beyond the Rust standard library.

## `Transcript` Type

`clank-transcript/src/lib.rs` defines `Transcript` as a bounded sliding window
backed by `VecDeque<String>`:

```rust
pub struct Transcript {
    entries: VecDeque<String>,
    max_entries: usize,
}
```

`VecDeque` is chosen over `Vec` because the sliding-window eviction pattern
requires `pop_front` on every `push` at capacity — O(1) with `VecDeque`,
O(n) with `Vec`.

### Public API

| Method | Behaviour |
|---|---|
| `new(max_entries: usize) -> Self` | Construct with given capacity. |
| `push(entry: impl Into<String>)` | Append entry; drop oldest if at capacity. |
| `clear()` | Discard all entries. |
| `trim(n: usize)` | Drop oldest `n` entries. `n = 0` is a no-op. `n ≥ len` clears all without error. |
| `entries() -> impl Iterator<Item = &str>` | Iterate entries oldest-first. |
| `len() -> usize` | Current entry count. |
| `is_empty() -> bool` | True if no entries. |

The default capacity is `DEFAULT_MAX_ENTRIES = 1000`. This constant is
`pub` so callers can reference it; the global is initialized with this value.

### Sliding-Window Semantics

When `push` is called on a full window, the oldest entry is silently evicted.
No error is returned and no notification is emitted. This is the correct
behaviour for a context window: the model always sees the most recent history,
and old history falls off automatically as the session grows. Token-budget
compaction (summarize-and-replace at the leading edge) is a separate future
concern that builds on this structural foundation.

## Process-Global Accessor

```rust
static GLOBAL: OnceLock<Arc<Mutex<Transcript>>> = OnceLock::new();

pub fn global() -> Arc<Mutex<Transcript>> {
    GLOBAL
        .get_or_init(|| Arc::new(Mutex::new(Transcript::new(DEFAULT_MAX_ENTRIES))))
        .clone()
}
```

The global is initialized on first access via `std::sync::OnceLock`. The
`Mutex` is `std::sync::Mutex` (not `tokio::sync::Mutex`) because the lock is
never held across an `.await` point in any production caller: recording in
`run_with_options` and `run_interactive` acquires the lock, pushes a `String`,
and releases it — all synchronously before the `.await` on `run_string`. The
`context` builtin is synchronous (`SimpleCommand::execute` is not async).

### Single-Shell Constraint

The process-global is correct for the current single-shell-per-process model.
If clank ever runs multiple independent shell instances in one process (e.g. a
test harness that wants isolated sessions), this will need revisiting. The
constraint is documented in the crate module doc.

## Recording Call Sites

Both `run_with_options` and `run_interactive` in `clank-core/src/lib.rs`
record the command text to the transcript immediately before calling
`shell.run_string`. This single pattern, applied at both call sites, makes
recording run-mode-agnostic:

```rust
clank_transcript::global()
    .lock()
    .unwrap_or_else(|e| e.into_inner())
    .push(command);
let result = shell.run_string(command, &params).await?;
```

`unwrap_or_else(|e| e.into_inner())` is used rather than `unwrap()` because a
poisoned `Mutex` — one whose last holder panicked — still contains valid
`VecDeque` data. Propagating the panic via `unwrap()` would cause a secondary
failure unrelated to the original problem.

Only the command text is recorded at this stage. Command output capture is
deferred: brush-core 0.4.0 exposes no output interception hook at the
`run_string` level. Recording command text is sufficient for the `context`
builtin surface and for all current acceptance tests.

## `context` Builtin

`ContextBuiltin` in `clank-builtins/src/lib.rs` implements
`brush_core::builtins::SimpleCommand`. It is registered in
`clank_core::default_options()`:

```rust
builtins.insert("context".to_owned(), clank_builtins::context_registration());
```

`context_registration()` returns `simple_builtin::<ContextBuiltin>()`.

### Subcommand Dispatch

The first argument after the command name is the subcommand. brush-core passes
the command name itself as `args[0]`, so `execute` skips the first element
before reading the subcommand.

| Invocation | Exit code | Effect |
|---|---|---|
| `context show` | 0 | Print all entries to stdout, one per line. Does not record its own output back. |
| `context clear` | 0 | Discard all entries. |
| `context trim <n>` | 0 | Drop oldest `n` entries. `n` must be a non-negative integer. |
| `context trim <invalid>` | 2 | Error to stderr. |
| `context trim` (no `n`) | 2 | Error to stderr. |
| `context <unknown>` | 2 | Error to stderr. |
| `context` (no subcommand) | 2 | Usage to stderr. |

### Non-Duplication of `context show` Output

`context show` cannot record its own output back into the transcript. The
recording call site is in `run_with_options`/`run_interactive`, which pushes
the command text *before* calling `run_string`. `context show` executes *inside*
`run_string`; its stdout output is written to the process's open file
descriptor 1, not through the recording path. No special guard is needed.

### Execution Scope

`context` is classified as `ShellInternal` in `MANIFEST_REGISTRY`. It is
registered as a brush builtin and therefore runs in the shell's own process,
with access to shell state. It cannot be invoked as a subprocess and is not
exposed to `ask` as an AI tool.

## Process-Group Fix for Interactive Mode

A separate bug discovered during implementation: external commands (`ls`, any
real subprocess) hung indefinitely when run in interactive mode
(`run_interactive` with `interactive_options()`).

Root cause: brush-core's `ProcessGroupPolicy` defaults to `NewProcessGroup`.
With `interactive: true`, brush puts each spawned subprocess into its own
process group and calls `tcsetpgrp` to hand it terminal foreground control.
clank's REPL loop does not implement terminal session management, so the
`tcsetpgrp` call races/hangs.

Fix: `run_interactive` sets `params.process_group_policy =
ProcessGroupPolicy::SameProcessGroup` before entering the loop. Subprocesses
inherit the shell's process group with no terminal foreground transfer. This
is correct for an embedded REPL that does not implement full job control.

## Test Coverage

### Tier 1 — Unit tests (`clank-transcript/src/lib.rs`)

11 tests covering: push ordering, sliding-window eviction at capacity, push
to capacity without eviction, repeated push beyond capacity, clear, clear
followed by push, and all trim variants (0, within bounds, exact length,
exceeding length).

### Tier 2 — Integration tests

**`clank-core/tests/transcript.rs`** (14 tests): recording via `run()`,
recording multiple commands, `context show/clear/trim` exit codes, `context
clear` verifiable via the global, `context trim` removing the oldest entry,
`context trim 0` no-op, error cases for invalid arguments and unknown
subcommands, recording via `run_interactive`, `context clear` in interactive
mode, and shared transcript between `run()` and `run_interactive()`.

Tests use `tokio::sync::Mutex` as a serialisation lock (`TEST_LOCK`) to
prevent races on the process-global transcript when tests run in parallel.

**`clank-core/tests/interactive.rs`** (15 tests): external command execution
(`ls`, `env`, `pwd`), failing external commands, external followed by builtin,
shell variable persistence across REPL lines, `cd` persistence (verified via
redirected `pwd` to a temp file), `$?` tracking, prompt output count, empty
line skipping, external pipeline (`ls /tmp | cat`), and semicolon-separated
command execution.

These tests are the primary regression guard for the process-group fix: if
`SameProcessGroup` is ever removed or the `interactive: true` path changes,
the external command tests will hang and time out rather than pass.

### Tier 3 — Acceptance tests (`clank-acceptance/cases/builtins/context.yaml`)

10 cases: `context show` contains recorded command text; `context clear`
followed by `context show` yields empty output; `context trim 1` removes the
only entry; `context trim 0` is a no-op; `context trim 999` clears all; `context
show` does not re-record its own output; `context trim <non-integer>` exits 2;
`context trim` with no argument exits 2; `context <unknown>` exits 2; `context`
with no subcommand exits 2.

## Deferred Work

- **Output capture** — command output is not yet recorded, only command text.
  brush-core 0.4.0 has no output interception hook at `run_string`. A future
  plan must address this to make the transcript useful as the AI's context
  window.
- **`context summarize`** — requires model access; separate issue.
- **Token-budget compaction** — summarize-and-replace at the leading edge;
  requires model access; separate issue. The `max_entries` cap is the
  structural foundation.
- **Redaction rules** — entries from commands with `redaction-rules` in their
  manifest should not enter the transcript; separate issue.
- **Golem oplog integration** — inside Golem the full uncompacted transcript
  is preserved in the component oplog; separate issue.
- **`ask` context consumption** — `ask` must receive the transcript window as
  its context; separate issue that depends on output capture.
