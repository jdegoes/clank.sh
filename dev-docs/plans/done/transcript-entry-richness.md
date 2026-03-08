---
title: "Transcript Entry Richness: typed entries, timestamps, and output capture"
date: 2026-03-07
author: agent
issue: "dev-docs/issues/open/transcript-entry-richness.md"
research:
  - "dev-docs/research/chrono-wasm32-wasip2-compatibility.md"
  - "dev-docs/research/brush-embedding-api.md"
designs:
  - "dev-docs/designs/proposed/transcript-and-context-builtin.md"
---

# Transcript Entry Richness: typed entries, timestamps, and output capture

## Originating Issue

Transcript records commands only — not their output, timestamp, or kind. See
`dev-docs/issues/open/transcript-entry-richness.md`.

## Research Consulted

- `dev-docs/research/chrono-wasm32-wasip2-compatibility.md` — confirms chrono
  is safe on `wasm32-wasip2`; recommended dependency declaration documented
  there.
- `dev-docs/research/brush-embedding-api.md` — brush-core 0.4.0 embedding
  API; `CreateOptions::fds` and `OpenFile::PipeWriter` are the output capture
  mechanism.

## Design

### New dependency: `chrono`

`clank-transcript/Cargo.toml` gains:

```toml
chrono = { version = "0.4", default-features = false, features = ["clock"] }
```

`default-features = false` prevents `wasmbind` from being pulled in via
feature unification. The `clock` feature enables `Utc::now()`. This is the
minimal correct configuration for a crate targeting both native and
`wasm32-wasip2` (see research doc).

### `TranscriptEntry` and `EntryKind` (`clank-transcript`)

The existing `VecDeque<String>` backing is replaced with
`VecDeque<TranscriptEntry>`. The `String` type is removed entirely.

```rust
pub struct TranscriptEntry {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub kind: EntryKind,
}

pub enum EntryKind {
    Command(String),   // command text typed or executed
    Output(String),    // captured stdout/stderr from a command
    AiResponse(String),// response from the model via ask (future use)
}
```

`TranscriptEntry` is constructed with `TranscriptEntry::new(kind)` which sets
`timestamp` to `Utc::now()`.

### Updated `Transcript` API

All existing methods are updated to operate on `TranscriptEntry` rather than
`String`. The public API surface becomes:

| Method | Change |
|---|---|
| `push(entry: TranscriptEntry)` | Replaces `push(entry: impl Into<String>)` |
| `clear()` | Unchanged |
| `trim(n: usize)` | Unchanged |
| `entries() -> impl Iterator<Item = &TranscriptEntry>` | Was `&str` |
| `len()` | Unchanged |
| `is_empty()` | Unchanged |

A convenience constructor is added:

```rust
impl TranscriptEntry {
    pub fn command(text: impl Into<String>) -> Self { ... }
    pub fn output(text: impl Into<String>) -> Self { ... }
    pub fn ai_response(text: impl Into<String>) -> Self { ... }
}
```

### Output capture mechanism

brush-core 0.4.0 exposes `CreateOptions::fds: Option<HashMap<ShellFd,
OpenFile>>` and `Shell::replace_open_files`. `OpenFile` is an enum that
includes `PipeWriter(std::io::PipeReader)`. `std::io::pipe()` (stable since
Rust 1.87) creates a `(PipeReader, PipeWriter)` pair.

**For `run_with_options`** (script mode):

1. Create a pipe: `let (reader, writer) = std::io::pipe()?`
2. Inject the writer as stdout fd 1 into `CreateOptions::fds` before
   `Shell::new`.
3. Call `shell.run_string(command, &params).await?`.
4. Drop/close the writer (now owned by the shell).
5. Read all output from the reader into a `String`.
6. Record a `TranscriptEntry::output(captured)` after the command entry.

**For `run_interactive`** (REPL mode):

The same pattern applies per-line. A new pipe is created before each
`run_string` call. The shell's stdout is replaced using
`shell.replace_open_files(...)` between iterations.

One subtlety: `run_interactive` takes an already-constructed `Shell`. We need
to replace its stdout fd before each `run_string` call and restore it
afterwards. `Shell::replace_open_files` replaces the shell's persistent open
files entirely; we must preserve stdin and stderr while swapping only stdout.
The implementation captures the current stdout, creates a pipe, replaces
stdout with the write end, runs the command, reads the pipe, then restores the
original stdout.

### `context show` output non-re-entry guarantee

With output capture in place, every command's stdout is captured and recorded
as an `Output` entry. `context show` and `context summarize` are
`shell-internal` builtins — they execute inside `run_string` and their output
goes to whatever stdout the shell currently has open. If that stdout is a
captured pipe, their output would be captured and recorded.

The exclusion mechanism: `run_with_options` and `run_interactive` check
whether the command text (trimmed) starts with `context show` or `context
summarize` before recording the captured output. If it does, the output entry
is discarded. The `Command` entry for the invocation is still recorded — only
the `Output` entry is suppressed.

This is a simple string prefix check at the recording call site. It is not a
general filter — it targets exactly the two commands the README names. If
`context summarize` is never invoked without output capture being active, the
check is a no-op.

**Rationale for prefix check over a flag:** A flag on `TranscriptEntry` or a
thread-local "suppress next output" boolean would be more indirect and harder
to audit. The prefix check at the recording call site is explicit, localised,
and covered directly by the acceptance tests.

### `context show` formatting

`context show` currently prints each entry as a plain `String`. With
`TranscriptEntry`, it formats each entry as:

```
[<timestamp>] <kind>: <text>
```

Example:

```
[2026-03-07T14:23:01Z] command: ls /tmp
[2026-03-07T14:23:01Z] output: 3aa57697...
[2026-03-07T14:23:02Z] command: echo hello
[2026-03-07T14:23:02Z] output: hello
```

The timestamp uses `to_rfc3339_opts(SecondsFormat::Secs, true)` — second
precision, UTC `Z` suffix. The kind tag (`command`, `output`, `ai_response`)
is the lowercase variant name.

### Downstream breakage

Three call sites in `clank-core` call `transcript.push(cmd)` with a `&str`.
These become `transcript.push(TranscriptEntry::command(cmd))`.

`context show` in `clank-builtins` iterates `locked.entries()` and calls
`writeln!(stdout, "{entry}")`. This becomes a formatted write per
`TranscriptEntry`.

The `transcript.rs` integration tests in `clank-core/tests/` call
`transcript_entries()` which returns `Vec<String>`. This becomes
`Vec<TranscriptEntry>` and assertions are updated to match on `entry.kind`.

The `clank-transcript` unit tests work directly with `Transcript` and push
plain strings; these are updated to push `TranscriptEntry` values.

## Developer Feedback

No open design questions. The output capture mechanism (`CreateOptions::fds` /
`replace_open_files` with a pipe) was confirmed by reading the brush-core
0.4.0 source directly. The `context show` / `context summarize` exclusion
mechanism (prefix check at recording call site) follows from the discussion
documented in the originating issue.

## Tasks

- [ ] Add `chrono` to `clank-transcript/Cargo.toml` with
      `default-features = false, features = ["clock"]`
- [ ] Add `TranscriptEntry` struct and `EntryKind` enum to `clank-transcript`,
      with `command()`, `output()`, `ai_response()` constructors setting
      `timestamp = Utc::now()`
- [ ] Replace `VecDeque<String>` with `VecDeque<TranscriptEntry>` in
      `Transcript`; update `push`, `entries`, and all internal uses
- [ ] Update unit tests in `clank-transcript` to use `TranscriptEntry`; assert
      on `kind` and verify `timestamp` is set (non-epoch)
- [ ] Update `context show` in `clank-builtins` to format each
      `TranscriptEntry` as `[<rfc3339>] <kind>: <text>`
- [ ] Update recording call sites in `clank-core/src/lib.rs`
      (`run_with_options`, `run_interactive`) to push
      `TranscriptEntry::command(cmd)` instead of a plain string
- [ ] Implement output capture in `run_with_options`: inject pipe as stdout
      via `CreateOptions::fds` before `Shell::new`; drain and record
      `TranscriptEntry::output(...)` after `run_string`; suppress output entry
      if command is `context show` or `context summarize`
- [ ] Implement output capture in `run_interactive`: replace shell stdout with
      pipe write end via `shell.replace_open_files(...)` before each
      `run_string`; drain and record output entry after; restore original
      stdout; suppress output entry for `context show` / `context summarize`
- [ ] Update `transcript.rs` integration tests in `clank-core/tests/` to
      assert on `TranscriptEntry` fields; add tests that verify output entries
      are recorded after commands; verify `context show` output is not recorded
- [ ] Update `context.yaml` acceptance tests: `context show` output now
      includes timestamp and kind prefix — update `expect_stdout_contains`
      assertions accordingly; add a case that verifies `context show` output
      itself does not appear as a subsequent entry
- [ ] Run full test suite; verify no regressions

## Acceptance Tests (additions / changes)

All in `clank-acceptance/cases/builtins/context.yaml`.

**Updated cases** (format change):

- `show_after_clear_is_empty` — unchanged; still expects `expect_stdout: ""`
- `show_output_is_not_re_recorded` — unchanged; still expects
  `expect_stdout: ""`

**New cases**:

- After `echo hello`, `context show` stdout must contain
  `command: echo hello` and `output: hello`
- `context show` output format includes a timestamp prefix matching RFC 3339
- `context show` output does not itself appear as a `command:` or `output:`
  line in a subsequent `context show` (non-re-entry guarantee)

## Out of Scope

- `AiResponse` entries — `ask` is not yet integrated; the variant exists in
  the type for completeness but is not produced by any current code path.
- `context summarize` — requires model access; separate issue.
- Token-budget compaction — separate issue.
- Redaction rules — separate issue.
- Golem oplog integration — separate issue.
- stderr capture — stdout only for this step; stderr follows the same
  mechanism and can be added later without API breakage.
