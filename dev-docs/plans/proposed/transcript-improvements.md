---
title: Transcript Improvements — Tee Streaming, Stderr Separation, Compaction
date: 2026-03-07
author: agent
issue: dev-docs/issues/open/transcript-tee-streaming.md
research: []
designs:
  - dev-docs/designs/approved/transcript.md
---

## Summary

Three targeted improvements to the existing transcript implementation, addressed
together because they all touch `transcript.rs` and `lib.rs`:

1. **Tee streaming** — output forwarded to terminal in real-time via a background
   drain thread, while simultaneously captured for the transcript.
2. **Stderr separation** — stdout and stderr captured on separate pipes and
   recorded as separate transcript entries (`Output` vs `Error`).
3. **Compaction** — token budget, sliding-window compaction, `Summary` entry kind,
   and `render()` with semantic `[input]/[output]/[error]/[ai]/[summary]` labels.

## Related Issues

- `dev-docs/issues/open/transcript-tee-streaming.md`
- `dev-docs/issues/open/transcript-stderr-separation.md`
- `dev-docs/issues/open/transcript-compaction.md`

## Developer Feedback

All three improvements derived from analysis of correctness and quality gaps in
the existing transcript implementation.

---

## Design

### 1. Tee Streaming (`clank/src/tee.rs` — new file)

A `CaptureHandle` backed by a background OS thread that drains the read end of
a pipe, forwards bytes to the real terminal in real-time, and accumulates them
in a buffer. After the command completes (write end dropped → EOF), `join()`
retrieves the captured text.

```rust
pub struct CaptureHandle {
    thread: std::thread::JoinHandle<String>,
}

impl CaptureHandle {
    pub fn join(self) -> String { ... }
}

pub fn capture_stdout() -> std::io::Result<(std::io::PipeWriter, CaptureHandle)>
pub fn capture_stderr() -> std::io::Result<(std::io::PipeWriter, CaptureHandle)>
```

`capture_stdout` forwards to `std::io::stdout()`, `capture_stderr` to
`std::io::stderr()`.

### 2. Stderr Separation (`clank/src/lib.rs`)

`run_command()` creates **two separate pipes** — one for stdout, one for stderr.
Each is captured independently via `CaptureHandle`. Both are recorded as
separate transcript entries.

`CommandOutcome` gains a separate `stderr` field:

```rust
pub struct CommandOutcome {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: u8,
}
```

The existing `output` field is replaced by `stdout`. This is a breaking change
to `CommandOutcome` — tests that reference `outcome.output` must be updated to
`outcome.stdout`.

### 3. Compaction (`clank/src/transcript.rs`)

#### New `TranscriptEntry` variants

```rust
pub enum TranscriptEntry {
    Command { input: String },
    Output { text: String },   // stdout
    Error { text: String },    // stderr (new)
    AiResponse { text: String },
    Summary { text: String },  // produced by compact() only (new)
}
```

#### Token budget and `Transcript::new(max_tokens)`

```rust
pub struct Transcript {
    entries: Vec<TranscriptEntry>,
    max_tokens: usize,
}

impl Transcript {
    pub fn new(max_tokens: usize) -> Self
    pub fn default_budget() -> Self  // max_tokens = 8_000
}
```

Token count approximation: `text.len() / 4` (1 token ≈ 4 characters).

#### Automatic compaction

After every `push_*` call, if `token_count() > max_tokens`, `compact()` fires:
- Replaces leading entries with a single `Summary` entry
- Targets 75% of `max_tokens` after compaction
- Always preserves at least the most recent entry

#### `render()` — semantic labels for LLM legibility

Replaces `as_string()` as the primary rendering method for `ask`:

```
[input] echo hello
[output] hello
[error] bash: foo: command not found
[ai] This is a shell session.
[summary] [summary of prior transcript]
```

`as_string()` is kept as an alias for backward compatibility with existing
`context show` tests.

---

## `CommandOutcome` Migration

| Old field | New field |
|---|---|
| `output: String` | `stdout: String` |
| (none) | `stderr: String` |

All tests and callers that reference `outcome.output` must be updated to
`outcome.stdout`.

---

## Acceptance Tests

1. `cargo test` passes — all existing tests updated and green.
2. A command that produces output over multiple steps displays each line as
   it is produced (tee test).
3. A command that writes to both stdout and stderr produces two separate
   transcript entries — `Output` and `Error`.
4. Empty stdout/stderr produces no transcript entry.
5. A transcript with 100-token budget and 101 tokens of entries automatically
   compacts, with a `Summary` entry at position 0.
6. The most recent entry is always preserved after compaction.
7. `render()` produces `[input]/[output]/[error]/[ai]/[summary]` labels.
8. `cargo clippy --all-targets -- -D warnings` passes.

## Tasks

- [ ] Create `clank/src/tee.rs` with `CaptureHandle`, `capture_stdout()`, `capture_stderr()`
- [ ] Add unit tests for `tee.rs`
- [ ] Update `TranscriptEntry` in `transcript.rs`: add `Error` and `Summary` variants
- [ ] Add `max_tokens` field to `Transcript`, implement `new(max_tokens)` and `default_budget()`
- [ ] Add `token_count()` approximation (`text.len() / 4`)
- [ ] Implement `compact()` with 75% target
- [ ] Add `push_error()` method to `Transcript`
- [ ] Rename `as_string()` → keep as alias; add `render()` with semantic labels
- [ ] Update unit tests in `transcript.rs` to cover `Error`, `Summary`, compaction, `render()`
- [ ] Update `CommandOutcome`: rename `output` → `stdout`, add `stderr`
- [ ] Update `run_command()` in `lib.rs` to use `tee.rs` capture, separate stdout/stderr pipes, record `Output` and `Error` separately
- [ ] Update all tests and callers that reference `outcome.output` → `outcome.stdout`
- [ ] Verify all acceptance tests pass
