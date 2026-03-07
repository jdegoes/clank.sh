---
title: Transcript Improvements — Realized Design
date: 2026-03-07
author: agent
---

## Overview

Three improvements to the transcript implementation, delivered together because
they all touch `transcript.rs` and `lib.rs`:

1. **Real-time output forwarding** — output reaches the terminal as it is
   produced, not buffered until the command ends.
2. **Stderr separation** — stdout and stderr recorded as separate transcript
   entries.
3. **Token budget and compaction** — transcript shrinks automatically when it
   approaches the context window limit.

---

## New File: `clank/src/tee.rs`

Provides a generic `pipe_and_capture(real_out)` function that creates an OS pipe
and a background drain thread. The thread reads bytes from the pipe, forwards
them to `real_out` immediately (real-time display), and accumulates them in a
buffer. When the write end is dropped (command finishes), the thread exits.
`StreamCapture::collect()` waits for the thread and returns the accumulated text.

```
Key types and functions:
  StreamCapture            — result of a capture operation; call .collect() to retrieve text
  pipe_and_capture(out)    — generic: takes any impl Write + Send as the real output
  tee_stdout()             — convenience: forwards to process stdout
  tee_stderr()             — convenience: forwards to process stderr
```

`pipe_and_capture` takes the real output as a parameter (not hardcoded to
`stdout`/`stderr`). This makes the function testable with `std::io::sink()` without
producing terminal output in tests.

---

## Updated: `clank/src/transcript.rs`

### New `TranscriptEntry` variants

```rust
pub enum TranscriptEntry {
    Command { input: String },
    Output { text: String },     // stdout
    Error { text: String },      // stderr (new)
    AiResponse { text: String },
    Summary { text: String },    // compacted history (new)
}
```

### Token budget and compaction

```rust
const WINDOW_COMPACTION_RATIO: f64 = 0.75;
const DEFAULT_TOKEN_BUDGET: usize = 8_000;

pub struct Transcript {
    entries: Vec<TranscriptEntry>,
    max_tokens: usize,
}
```

Key methods:
- `Transcript::new(max_tokens)` — explicit budget
- `Transcript::with_default_budget()` — 8,000 tokens
- `approximate_tokens() -> usize` — `text.len() / 4`, minimum 1 per entry
- `compact()` — called automatically after every `push_*`; drops leading entries
  until `summary_tokens + remaining_tokens ≤ WINDOW_COMPACTION_RATIO * max_tokens`;
  inserts a fixed `Summary` placeholder at position 0; always preserves the most
  recent entry
- `push_error(&str)` — records stderr; empty text is ignored

### Two rendering methods

`format_for_model() -> String` — semantic labels for AI consumption:
```
[input] echo hello
[output] hello
[error] bash: foo: not found
[ai] This is a shell.
[summary] [earlier transcript compacted]
```

`as_string() -> String` — plain shell format for human display (`context show`):
```
$ echo hello
hello
```

### `CommandOutcome`

```rust
pub struct CommandOutcome {
    pub stdout: String,   // was `output`
    pub stderr: String,   // new
    pub exit_code: u8,
}
```

---

## Updated: `clank/src/lib.rs`

`run_command()` now:
1. Calls `tee::tee_stdout()` and `tee::tee_stderr()` to create two separate capture pairs.
2. Installs each as fd 1 and fd 2 on `ExecutionParameters`.
3. After the command, calls `stdout_capture.collect()` and `stderr_capture.collect()`.
4. Records `push_output(&stdout)` and `push_error(&stderr)` as separate entries.
5. Returns `CommandOutcome { stdout, stderr, exit_code }`.

`transcript_as_string()` now returns `transcript.format_for_model()` (labelled
format) rather than `as_string()` (plain format). `context show` still calls
`as_string()` for human-readable display.

---

## Compaction Design Note

The `Summary` entry contains a fixed placeholder string rather than a verbatim
copy of the absorbed entries. This keeps the summary size bounded and predictable.
A future `context summarize` command (requiring `ask`/HTTP) will generate a
meaningful AI summary to replace it.

---

## Test Coverage

| Layer | Count | What |
|---|---|---|
| Unit — `tee.rs` | 5 | `pipe_and_capture` with `sink()`, empty pipe, accumulated writes, tee_stdout, tee_stderr |
| Unit — `transcript.rs` | 17 | All `TranscriptEntry` variants, `push_*` methods, `clear`, `trim`, `format_for_model`, `as_string`, `approximate_tokens`, compaction (3 tests) |
| Unit — `lib.rs` | 10 | `ClankShell` construction, stdout/stderr separation, exit codes, context operations |
| Integration + system | unchanged | All 51 prior integration/system tests still pass |

**Total: 32 lib unit tests + all integration tests passing. Clippy clean.**

---

## Deviations from the Approved Plan

None. All tasks completed as specified.
