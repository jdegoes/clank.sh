---
title: Transcript and context Builtin — Realized Design
date: 2026-03-07
author: agent
---

## Overview

This document records the transcript implementation as actually built. It
supersedes any prior approved design for this area (none existed).

---

## What Was Built

A `Transcript` data structure owned by a new `ClankShell` wrapper. The REPL
captures every command input and its output into the transcript. `context`
subcommands are intercepted directly in the REPL — not registered as Brush
builtins — because they are `shell-internal` scoped per the README and operate
on shell-owned state. A public `transcript_as_string()` method provides the
interface `ask` will use.

---

## Module Structure

```
clank/src/
├── lib.rs           ← ClankShell, build_shell(), run_repl()
├── transcript.rs    ← Transcript, TranscriptEntry, CommandOutcome (new)
└── main.rs          ← unchanged (thin 4-line entry point)
```

---

## Types

### `TranscriptEntry` (`transcript.rs`)

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum TranscriptEntry {
    Command { input: String },
    Output { text: String },
    AiResponse { text: String },   // populated by `ask` (future task)
}
```

### `CommandOutcome` (`transcript.rs`)

```rust
pub struct CommandOutcome {
    pub output: String,    // combined stdout + stderr captured from the command
    pub exit_code: u8,
}
```

### `Transcript` (`transcript.rs`)

```rust
pub struct Transcript {
    entries: Vec<TranscriptEntry>,   // ordered, private
}
```

Public methods:
- `push_command(&mut self, input: &str)` — record a command line
- `push_output(&mut self, text: &str)` — record output; empty strings ignored
- `push_ai_response(&mut self, text: &str)` — for `ask` (future)
- `clear(&mut self)` — discard all entries
- `trim(&mut self, n: usize)` — drop oldest n entries; saturates at len
- `as_string(&self) -> String` — render as `$ cmd\noutput\n...`
- `is_empty(&self) -> bool`
- `len(&self) -> usize`

### `ClankShell` (`lib.rs`)

```rust
pub struct ClankShell {
    shell: Shell,            // brush_core::Shell — private
    transcript: Transcript,  // session memory — private (but accessible in tests via pub fields)
}
```

Public methods:
- `new() -> Self` (async) — constructs shell, registers clank-builtins
- `run_command(&mut self, input: &str) -> CommandOutcome` (async)
- `context_show(&self) -> String` — returns transcript as string; NOT recorded
- `context_clear(&mut self)` — clears transcript
- `context_trim(&mut self, n: usize)` — drops oldest n entries
- `transcript_as_string(&self) -> String` — for `ask`
- `last_result(&self) -> u8` — exit code of last command
- `run_string_raw(...)` — bypass transcript, for `clank-golden` setup scripts
- `default_exec_params(&self)` — for `clank-golden`

---

## `build_shell()` and `run_repl()`

`build_shell() -> ClankShell` — delegates to `ClankShell::new()`.

`run_repl(mut shell: ClankShell)` intercepts `context` commands before
dispatching to `run_command()`:

```
"exit"              → break
"context show"      → print context_show(); NOT recorded in transcript
"context clear"     → context_clear()
"context trim <n>"  → context_trim(n); parse error → stderr message
_                   → run_command(trimmed).await
```

---

## Output Capture

`run_command()` uses `std::io::pipe()` to capture stdout and stderr:

1. `io::pipe()` → `(PipeReader, PipeWriter)`. Writer cloned for stderr.
2. `params.set_fd(1, OpenFile::PipeWriter(writer))` — stdout to pipe.
3. `params.set_fd(2, OpenFile::PipeWriter(writer_clone))` — stderr to pipe.
4. `shell.run_string(input, &params).await`.
5. `drop(params)` — closes write ends.
6. `reader.read_to_string(&mut output)` — drain captured output.
7. `print!("{output}")` — write to real terminal.
8. `transcript.push_command(input)` + `transcript.push_output(&output)`.

Output is flushed to the terminal after command completion (not streamed).
Streaming (tee) is deferred — a future improvement.

---

## `context show` Non-Recording Invariant

Per README: output of `context show` must NOT be recorded back into the
transcript. This is enforced architecturally: `context_show()` is called
directly in the REPL, its return value is printed to stdout, and the REPL
does NOT call `run_command()` for it — so no transcript recording occurs.

This is verified by the unit test `context_show_does_not_grow_transcript`
and the integration test `context_show_does_not_record_itself`.

---

## `clank-golden` Compatibility

`clank-golden` previously held a direct `brush_core::Shell`. It now uses
`ClankShell` via two escape-hatch methods:
- `run_string_raw()` — runs a command on the inner shell without capturing
  or recording. Used for setup scripts.
- `default_exec_params()` — returns params for manual pipe-based capture.

---

## Test Coverage

| Layer | Count | What |
|---|---|---|
| Unit — `transcript.rs` | 10 | All `Transcript` methods |
| Unit — `lib.rs` | 9 | `ClankShell` construction, `run_command`, all `context_*` methods |
| Integration — `repl.rs` | 4 | `context show`, `context clear`, `context trim`, non-recording invariant |
| System — `system.rs` | 3 | Multi-step transcript scenarios |

**Total: 75 tests, all passing. Clippy clean.**

---

## Deviations from the Approved Plan

- The plan said `context trim` parses `n` and calls `context_trim(n)`. This
  was implemented exactly as specified. An invalid argument prints an error
  to stderr rather than panicking — a small robustness improvement.
- The `clank-builtins` unit tests were updated to use `ClankShell::run_command()`
  instead of calling `register()` + `run_string()` directly — they now test
  through the same API surface as production code.
