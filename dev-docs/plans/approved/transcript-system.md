---
title: "Transcript system — sliding-window session history and AI context window"
date: 2026-03-06
author: agent
issue: dev-docs/issues/open/transcript-system.md
research: []
designs: []
---

## Summary

Implement a `Transcript` type in `clank-core` that records every entry appended to the shell
session, maintains a sliding window bounded by a configurable token budget, and exposes a read
view suitable for use as an AI context window. Wire the `Repl` loop to record every input line
and (captured) command output automatically.

## Design Decisions

### Entry types

The transcript is an ordered sequence of `Entry` values. Each entry has a kind and text content:

```rust
pub enum EntryKind {
    Input,      // a line typed by the user
    Output,     // text produced by a command (stdout)
    Error,      // text produced by a command (stderr)
    AiResponse, // a response from the model (future use)
    Summary,    // a compacted summary block replacing older entries
}

pub struct Entry {
    pub kind: EntryKind,
    pub text: String,
}
```

`Summary` entries are never appended by the caller — they are produced internally by the
compaction logic. All other kinds are appended via the public API.

### Token budget and compaction

The transcript tracks an approximate token count using a simple heuristic: 1 token ≈ 4 characters
(sufficient for planning purposes; a real tokenizer is a future concern). When `append()` would
push the total over `max_tokens`, the transcript compacts the leading edge: the oldest non-Summary
entries are replaced with a single `Summary` entry whose text is their concatenated content
prefixed with `[summary of prior transcript]`. This keeps the boundary between summarized and live
history explicit, as described in the README.

Compaction target: reduce to 75% of `max_tokens` when triggered, creating headroom for future
entries. This avoids compacting on every single append when near the limit.

**Default `max_tokens`:** 8,000 (a conservative default; configurable at construction time).
This is intentionally well below a real model's context window — the real limit will be set by
the provider layer when `ask` is implemented.

### Output capture

brush-core exposes a `Stream` trait (`brush_core::openfiles::Stream: Read + Write + Send + Sync`)
and an `OpenFile::Stream(Box<dyn Stream>)` variant. The shell's fd table is accessible via
`shell.open_files_mut()` and `ExecutionParameters` carries per-command open files settable via
`params.open_files.set_fd(fd, file)`.

`try_clone_to_owned()` on the `Stream` trait is called only when brush-core spawns **external**
processes — it needs a real OS fd to pass to the child. For builtins it is never called. This
means a tee stream must be backed by a real OS pipe to support external commands like `echo`.

**Approach:** Before each `run_string`, create an OS pipe (`std::io::pipe()`). Wrap the write
end in a `TeeStream` that writes to both the pipe and the real stdout (so output is still visible
to the user). Pass the write end via `params.open_files.set_fd(STDOUT_FD, ...)`. After
`run_string` completes, close the write end, drain the read end, and append the captured bytes to
the transcript as an `Output` entry. Do the same for stderr → `Error` entry.

A separate `tee.rs` module in `clank-core` implements `TeeStream`:

```rust
pub struct TeeStream {
    write_end: std::io::PipeWriter,   // also cloned as OwnedFd for external procs
    passthrough: Box<dyn std::io::Write + Send + Sync>,  // real stdout/stderr
}
```

`try_clone_to_owned` delegates to the `PipeWriter`'s underlying fd.
`try_borrow_as_fd` does the same.

This is fully implementable with no new dependencies and no process model redesign.

### `Transcript` API

```rust
impl Transcript {
    pub fn new(max_tokens: usize) -> Self;
    pub fn append(&mut self, kind: EntryKind, text: impl Into<String>);
    pub fn entries(&self) -> &[Entry];
    pub fn token_count(&self) -> usize;
    pub fn clear(&mut self);
    pub fn trim(&mut self, n: usize);  // drop oldest n entries
    pub fn render(&self) -> String;    // for passing to a model as context
}
```

`render()` returns the full window as a plain string, with each entry formatted as:

```
[input] echo hello
[output] hello
[summary] [summary of prior transcript]
...
```

This format is designed to be readable to an LLM with no additional prompt engineering.

### Repl integration

`Repl` gains an `Arc<Mutex<Transcript>>` field. It is constructed with a default transcript
(8,000 token budget). Callers who need access to the transcript (e.g. future `context` builtin
tests) can pass one in via `Repl::with_transcript(transcript)`.

In `run()`, for each non-empty input line:
1. `transcript.append(EntryKind::Input, line)` is called.
2. OS pipes are created for stdout and stderr.
3. `TeeStream` wrappers are installed on the `ExecutionParameters`.
4. `run_string` is called.
5. The write ends are dropped, the read ends are drained, and captured output is appended
   as `Output` and `Error` entries respectively.

### New module layout

```
clank-core/src/
  lib.rs              (re-exports Repl, Transcript, Entry, EntryKind)
  repl.rs             (existing; gains transcript field and TeeStream wiring)
  transcript.rs       (new; Transcript, Entry, EntryKind)
  tee.rs              (new; TeeStream implementing brush_core::openfiles::Stream)
```

No new crates. No new third-party dependencies — token counting uses a character heuristic
implemented directly. The `TeeStream` implementation uses `std::io::pipe()` (stable since Rust
1.83) and `std::os::fd::OwnedFd` (stable on Unix).

## Developer Feedback

Design decisions discussed with developer:

- **Entry types:** Input, Output, Error, AiResponse, Summary — agreed.
- **Token counting:** character heuristic (÷4) for now; real tokenizer is future work.
- **Output capture:** implement fully via `TeeStream` + OS pipes; not deferred. Developer
  questioned the original deferral after reviewing brush-core's `Stream` trait and `open_files_mut`
  API — capture is straightforward and should be included in this task.
- **API shape:** `append` / `entries` / `render` / `clear` / `trim` — agreed.
- **Repl integration:** `Arc<Mutex<Transcript>>` field; `with_transcript` constructor — agreed.

## Acceptance Tests

1. `cargo build` exits 0.
2. `cargo test` exits 0 with no failures.
3. Unit tests verify:
   - Appending entries records them in order.
   - `render()` produces correctly formatted output.
   - Compaction fires when token budget is exceeded and produces a Summary entry.
   - `clear()` empties the transcript.
   - `trim(n)` drops the oldest n entries.
4. Integration test: `Repl` driven with `echo hello`; transcript contains one `Input` entry
   with `"echo hello"` and one `Output` entry containing `"hello"`.
5. Golden test: add a golden case that verifies `echo hello` still produces `hello` on stdout
   (i.e. the tee stream does not suppress visible output).

## Tasks

- [x] Create `dev-docs/issues/open/transcript-system.md`
- [x] Create `dev-docs/plans/proposed/transcript-system.md` (this file)
- [ ] Create `clank-core/src/transcript.rs` — `Entry`, `EntryKind`, `Transcript`
- [ ] Create `clank-core/src/tee.rs` — `TeeStream` implementing `brush_core::openfiles::Stream`
- [ ] Re-export `Transcript`, `Entry`, `EntryKind` from `clank-core/src/lib.rs`
- [ ] Add `Arc<Mutex<Transcript>>` field to `Repl`; add `Repl::with_transcript` constructor
- [ ] Wire `TeeStream` into `Repl::run()` for stdout and stderr capture
- [ ] Record input lines and captured output/error in `Repl::run()`
- [ ] Unit tests in `transcript.rs` covering all acceptance criteria
- [ ] Integration test in `clank-core/tests/repl_integration.rs`: `echo hello` → Input + Output entries
- [ ] Verify existing golden tests still pass (tee stream must not suppress visible output)
- [ ] Verify `cargo build` and `cargo test` pass
