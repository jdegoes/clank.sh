---
title: Implement Transcript and context Builtin
date: 2026-03-07
author: agent
issue: dev-docs/issues/open/transcript.md
research:
  - dev-docs/research/transcript.md
designs: []
---

## Summary

Introduce the `Transcript` data structure as a first-class value owned by a new
`ClankShell` wrapper. The REPL captures every command input and its output into
the transcript. `context show`, `context clear`, and `context trim` are handled
directly by the REPL — not as Brush builtins — because they are `shell-internal`
scoped per the README and operate on shell-owned state. A public
`transcript_as_string()` method on `ClankShell` provides the interface `ask`
will use.

## Developer Feedback

- `context` must NOT be a Brush builtin — it is `shell-internal` scoped per
  the README. It operates on shell-internal tables, not as a subprocess.
- No global mutable state (`OnceCell`, `thread_local` etc.) — rejected.
- No `Arc<Mutex<T>>` passed through Brush's registration system — rejected.
- `ClankShell` owns the transcript. The REPL intercepts `context` commands
  directly. This is the cleanest, most testable, architecturally honest design.

## Architecture

```
ClankShell
├── shell: brush_core::Shell        ← scripting layer (unchanged)
└── transcript: Transcript          ← session memory (new)

REPL loop intercepts:
  "exit"              → handled directly (existing)
  "context show"      → ClankShell::context_show() — output NOT recorded
  "context clear"     → ClankShell::context_clear()
  "context trim <n>"  → ClankShell::context_trim(n)
  everything else     → ClankShell::run_command() → capture → transcript
```

## New Types

### `Transcript` (in `clank/src/transcript.rs`)

```rust
/// A single entry in the shell transcript.
pub enum TranscriptEntry {
    /// A command typed by the operator (human or AI).
    Command { input: String },
    /// Output produced by a command (stdout + stderr combined, as rendered).
    Output { text: String },
    /// A response from an AI model (populated by `ask` — future task).
    AiResponse { text: String },
}

/// The shell's session transcript — a first-class value owned by ClankShell.
pub struct Transcript {
    entries: Vec<TranscriptEntry>,
}

impl Transcript {
    pub fn new() -> Self
    pub fn push_command(&mut self, input: &str)
    pub fn push_output(&mut self, text: &str)
    pub fn clear(&mut self)
    pub fn trim(&mut self, n: usize)       // drop oldest n entries
    pub fn as_string(&self) -> String      // render full transcript
    pub fn is_empty(&self) -> bool
}
```

`as_string()` renders the transcript in a format useful for `ask`:
```
$ echo hello
hello
$ ls
Cargo.toml
clank
```

### `ClankShell` (replaces bare `Shell` in `clank/src/lib.rs`)

```rust
pub struct ClankShell {
    shell: brush_core::Shell,
    transcript: Transcript,
}

impl ClankShell {
    pub async fn new() -> Self
    pub fn context_show(&self) -> String           // returns transcript as string
    pub fn context_clear(&mut self)                // clears transcript
    pub fn context_trim(&mut self, n: usize)       // drops oldest n entries
    pub fn transcript_as_string(&self) -> String   // for ask
    pub async fn run_command(&mut self, input: &str) -> CommandOutcome
}
```

### `CommandOutcome` (named type — no anonymous tuples)

```rust
/// The outcome of running a single command through ClankShell.
pub struct CommandOutcome {
    /// Combined stdout + stderr output captured from the command.
    pub output: String,
    /// Exit code of the command.
    pub exit_code: u8,
}
```

## Output Capture Strategy

In `run_command`:
1. Create a `std::io::pipe()` pair.
2. Set both stdout (fd 1) and stderr (fd 2) to `OpenFile::PipeWriter` on the params.
3. Call `shell.run_string(input, &params).await`.
4. Drop params to close the write ends.
5. Read captured output from the pipe reader.
6. Write captured output to real stdout (the process's actual stdout).
7. Append `TranscriptEntry::Command { input }` and `TranscriptEntry::Output { text }` to transcript.

This means output is flushed to the terminal after the command completes rather
than streaming. This is correct for this task. Streaming (tee) is a future
improvement.

## REPL Changes

`run_repl` takes a `ClankShell` instead of `brush_core::Shell`. The REPL loop
handles `context` commands directly before falling through to `run_command`:

```rust
match trimmed {
    "exit" => break,
    "context show" => {
        let text = clank_shell.context_show();
        println!("{text}");
        // NOT recorded into transcript
    }
    "context clear" => clank_shell.context_clear(),
    s if s.starts_with("context trim ") => {
        // parse n, call context_trim(n)
    }
    _ => {
        let outcome = clank_shell.run_command(trimmed).await;
        // output already written to terminal and recorded in transcript
    }
}
```

## `build_shell` Changes

`build_shell()` now returns `ClankShell` instead of `brush_core::Shell`.
Internal construction of `brush_core::Shell` moves inside `ClankShell::new()`.
The public signature becomes:

```rust
pub async fn build_shell() -> ClankShell
```

## Acceptance Tests

1. `cargo test` passes — all 54 existing tests still green.
2. Unit tests on `Transcript` cover: push_command, push_output, clear, trim, as_string.
3. Unit tests on `ClankShell` cover: run_command records input and output, context_show
   returns the transcript, context_clear empties it, context_trim drops oldest entries.
4. Integration test: run several commands, `context show` prints all of them.
5. Integration test: `context clear` followed by `context show` prints nothing.
6. Integration test: `context trim 1` drops the first command from the transcript.
7. `context show` output is NOT itself recorded back into the transcript.
8. `cargo clippy --all-targets -- -D warnings` passes.

## Module Structure

```
clank/src/
├── lib.rs           ← ClankShell, build_shell(), run_repl()
├── transcript.rs    ← Transcript, TranscriptEntry, CommandOutcome (new)
└── main.rs          ← unchanged (thin entry point)
```

## Tasks

- [ ] Create `clank/src/transcript.rs` with `TranscriptEntry`, `Transcript`, `CommandOutcome`
- [ ] Add unit tests for `Transcript` in `transcript.rs` under `#[cfg(test)]`
- [ ] Create `ClankShell` struct in `clank/src/lib.rs` wrapping `brush_core::Shell` + `Transcript`
- [ ] Implement `ClankShell::new()`, `run_command()`, `context_show()`, `context_clear()`, `context_trim()`, `transcript_as_string()`
- [ ] Update `build_shell()` to return `ClankShell`
- [ ] Update `run_repl()` to take `ClankShell`, intercept `context` commands, use `run_command()`
- [ ] Update `main.rs` if needed (should remain unchanged — calls `build_shell()` and `run_repl()`)
- [ ] Add unit tests for `ClankShell` in `lib.rs` under `#[cfg(test)]`
- [ ] Add integration tests to `clank/tests/repl.rs` for `context show`, `context clear`, `context trim`
- [ ] Add system test to `clank/tests/system.rs`: scenario_transcript_records_commands
- [ ] Verify all acceptance tests pass: `cargo test` and `cargo clippy`
