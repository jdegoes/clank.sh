---
title: Transcript Implementation — Data Structure, Capture Mechanism, and context Builtin
date: 2026-03-07
author: agent
---

## Purpose

Determine the correct data structure for the transcript, how to capture command
input and output into it during the REPL loop, and how the `context` builtin can
access transcript state that lives outside `brush_core::Shell`.

---

## Finding 1: brush_core::Shell Has No Transcript

`brush_core::Shell` has a `history()` method — that is bash command history
(`.bash_history`), not the clank transcript. The transcript is entirely clank's
own concept and must be owned by clank, not delegated to Brush.

---

## Finding 2: The `ClankShell` Wrapper Pattern

The transcript cannot be stored inside `brush_core::Shell` — Brush has no
extension point for arbitrary shell-level state. The correct pattern is a
`ClankShell` wrapper struct that owns both:

```rust
pub struct ClankShell {
    /// The underlying bash-compatible interpreter.
    shell: brush_core::Shell,
    /// The transcript of all rendered terminal I/O for this session.
    transcript: Transcript,
}
```

`ClankShell` becomes the primary API surface — `build_shell()` returns a
`ClankShell`, `run_repl()` takes a `ClankShell`. The `brush_core::Shell` is
an internal detail.

---

## Finding 3: Transcript Data Structure

The transcript is an ordered list of entries. Each entry is one of:

```rust
/// A single entry in the shell transcript.
pub enum TranscriptEntry {
    /// A command typed by the operator (human or AI).
    Command { input: String },
    /// Output produced by a command (stdout + stderr combined as rendered).
    Output { text: String },
    /// A response from the AI model (future — added by `ask`).
    AiResponse { text: String },
}

/// The shell's session transcript — a first-class value owned by ClankShell.
pub struct Transcript {
    entries: Vec<TranscriptEntry>,
}
```

Methods needed for this task:
- `push_command(input: &str)` — record a command line
- `push_output(text: &str)` — record rendered output
- `clear()` — discard all entries
- `trim(n: usize)` — drop oldest n entries
- `as_string() -> String` — render full transcript for `ask`

---

## Finding 4: Output Capture in the REPL

The REPL currently calls `shell.run_string(line, &params)` with default params
— output goes directly to the process's stdout (fd 1). To capture output into
the transcript, we need to intercept it.

The mechanism is already proven in `clank-golden`:

```rust
use std::io;
use brush_core::openfiles::OpenFile;

let (mut reader, writer) = io::pipe()?;
let mut params = shell.default_exec_params();
params.set_fd(1, OpenFile::PipeWriter(writer));
params.set_fd(2, OpenFile::PipeWriter(writer_clone));

shell.run_string(line, &params).await?;
drop(params); // close write end

let mut captured = String::new();
reader.read_to_string(&mut captured)?;
```

However, there is a problem with this approach for the REPL: the output is
fully buffered and only available after the command completes. For the initial
implementation this is acceptable — streaming to the terminal simultaneously
while capturing is a future concern (requires a tee-like mechanism).

**Approach for this task:** capture output into the transcript AND write it to
real stdout, using a two-step approach:
1. Capture to a `String` buffer via pipe
2. Write the buffer to real stdout (terminal)
3. Append the buffer to the transcript

This means the user sees output after the command completes rather than
streaming. For long-running commands this is a regression — but it is correct
for the initial implementation and can be improved later with a tee approach.

---

## Finding 5: How `context` Accesses the Transcript — The Key Challenge

`context` is registered as a `brush_core::builtins::Command`. Its `execute`
method receives `ExecutionContext<'_>` which gives access to `context.shell`
(a `&mut brush_core::Shell`). But the transcript lives in `ClankShell`, not
in `brush_core::Shell`.

There are three options:

### Option A: Store transcript in a shell environment variable

Store the transcript as a `ShellVariable` in `shell.env`. `context` reads/writes
it via `shell.env`. Simple, no new abstractions.

**Problem:** `ShellVariable` is designed for string values visible to bash scripts.
Storing the full session transcript as a shell variable is semantically wrong —
it would be visible to scripts, could be accidentally overwritten, and the type
system provides no protection.

### Option B: Store transcript in a thread-local / global

Use a thread-local `RefCell<Transcript>` accessible from both the REPL and the
`context` builtin.

**Problem:** Thread-locals are awkward, not WASM-friendly (WASM is single-threaded
but thread-locals are still compiler-dependent), and hide state in a non-obvious
location.

### Option C: Store transcript behind an `Arc<Mutex<Transcript>>`

The `ClankShell` wrapper holds `Arc<Mutex<Transcript>>`. A clone of the same
`Arc` is given to `ContextCommand` at registration time (stored inside the
builtin registration struct). Both the REPL and `context` share the same
transcript via the `Arc`.

```rust
let transcript = Arc::new(Mutex::new(Transcript::new()));
// REPL uses transcript.clone() to append entries
// ContextCommand holds transcript.clone() to read/clear/trim
```

**This is the correct approach.** `Arc<Mutex<T>>` is the standard Rust pattern
for shared mutable state across async contexts. It is WASM-compatible. It makes
the dependency explicit and the type safe. `brush_core::builtins::Registration`
can store arbitrary state — the `context` builtin can hold its own `Arc` clone.

---

## Finding 6: `context` Registration with Shared State

`brush_core::builtins::builtin::<T>()` requires `T: Parser + Command + Default`.
The `Default` derive is needed. But `ContextCommand` needs to hold the transcript
`Arc` — it cannot implement `Default` in a meaningful way.

Looking at `brush_core::builtins::Registration` more carefully: `register_builtin`
takes a `Registration` value, not a type parameter. The `builtin::<T>()` function
is a convenience that creates a `Registration` from a `T: Default + Command`.

For `context`, we can create the `Registration` manually — passing a factory
closure or a pre-built instance that already holds the `Arc`. This is the
correct path.

Alternatively: store the `Arc<Mutex<Transcript>>` in a `static` or use a
`once_cell` for the initial implementation. But the `Arc` approach is cleaner.

The simplest implementation: use `brush_core::builtins::builtin::<T>()` but
have `ContextCommand` access the transcript via a `static` `once_cell::sync::OnceCell<Arc<Mutex<Transcript>>>` initialised at shell construction time. This avoids the `Default` + `Arc` registration complexity.

**Selected approach:** `once_cell::sync::OnceCell<Arc<Mutex<Transcript>>>` as a
module-level static in `clank-builtins`. Initialised once by `register()`.
WASM-compatible. Simple. Avoids needing to customise `Registration`.

---

## Finding 7: `context` as a Subcommand

`context` has subcommands: `show`, `clear`, `trim`. The natural Rust
implementation is a single `ContextCommand` with a `#[command(subcommand)]`
enum:

```rust
#[derive(Parser)]
pub struct ContextCommand {
    #[command(subcommand)]
    subcommand: ContextSubcommand,
}

#[derive(Subcommand)]
enum ContextSubcommand {
    Show,
    Clear,
    Trim { n: usize },
}
```

---

## Finding 8: `context show` Must Not Re-record Itself

Per README: "`context show` and `context summarize` are transcript-inspection
commands: their output is written to stdout but is NOT recorded back into the
transcript."

In our architecture, recording happens in the REPL loop after `run_string`
returns. The REPL must check whether the command that just ran was `context show`
or `context summarize` and skip recording the output in that case.

Simpler: `context show` sets a flag on the transcript (`suppress_next_output:
bool`) that the REPL checks before appending the captured output. The `context`
builtin sets this flag; the REPL reads and clears it.

---

## Conclusions

1. **`ClankShell` wrapper** — owns `brush_core::Shell` + `Arc<Mutex<Transcript>>`
2. **`Transcript`** — `Vec<TranscriptEntry>` with `Command`, `Output`, `AiResponse` variants
3. **Output capture** — pipe-based capture in REPL; write to terminal + append to transcript
4. **Shared state** — `once_cell::sync::OnceCell<Arc<Mutex<Transcript>>>` in `clank-builtins`
5. **`context` builtin** — `ContextCommand` with `show`/`clear`/`trim` subcommands via clap
6. **`context show` suppression** — `suppress_next_output` flag on `Transcript`
7. **Public `transcript_as_string()`** — on `ClankShell`, for use by `ask`
