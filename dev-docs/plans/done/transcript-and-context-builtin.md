---
title: "Transcript Data Structure and context Builtin"
date: 2026-03-07
author: agent
issue: "dev-docs/issues/open/transcript-and-context-builtin.md"
research:
  - "dev-docs/research/brush-embedding-api.md"
designs: []
---

# Transcript Data Structure and context Builtin

## Originating Issue

No transcript data structure or `context` builtin — transcript exists only
conceptually. See `dev-docs/issues/open/transcript-and-context-builtin.md`.

## Research Consulted

`dev-docs/research/brush-embedding-api.md` — brush-core 0.4.0 embedding API.
The critical finding is documented below under **Design Constraint**.

## Design Constraint: brush-core builtin state access

The brush-core 0.4.0 builtin registration API stores builtins as bare `fn`
pointers:

```rust
pub type CommandExecuteFunc = fn(
    commands::ExecutionContext<'_>,
    Vec<commands::CommandArg>,
) -> BoxFuture<'_, Result<results::ExecutionResult, error::Error>>;
```

This is a `fn` pointer, not a `Box<dyn Fn>` or any closure-bearing type.
`Registration` has no user-data field. `Shell` has no extension slot. The
`ExecutionContext` gives the builtin `pub shell: &'a mut Shell` — a concrete
brush-core type with no clank-owned fields.

**Consequence:** the `context` builtin's execute function cannot receive an
`Arc<Mutex<Transcript>>` through the normal registration path. The only
channel through which a brush-core builtin can access clank-owned state that
is not serializable as a shell variable is a process-level shared reference.

**Chosen approach:** the `Transcript` is stored as a process-global
`Arc<Mutex<Transcript>>`, initialized once at shell startup inside
`clank-core`. Both the recording call site (wrapping `shell.run_string`) and
the `context` builtin access it through this global. This is correct for the
current single-shell-per-process model and is documented explicitly as a
constraint. If clank ever runs multiple shells in one process, this will need
revisiting.

## Developer Feedback

Two design decisions confirmed:

1. **Process-global `Arc<Mutex<Transcript>>`** — correct approach for the
   current single-shell-per-process model. Document the constraint explicitly.

2. **Sliding window from the start** — `Transcript` is a bounded sliding
   window, not an unbounded list. It must enforce a maximum entry count on
   every `push` and drop oldest entries automatically when the cap is reached.
   Compaction (summarization at the token-budget boundary) is deferred, but
   the structural capacity constraint must be present from the beginning.

## Approach

### New crate: `clank-transcript`

The `Transcript` type and the process-global accessor must live in a
dedicated crate, `clank-transcript`, that both `clank-core` and
`clank-builtins` depend on. This is not optional:

`clank-core` already depends on `clank-builtins`. If `clank-builtins`
depended on `clank-core` to reach the transcript global, the dependency
graph would be cyclic. `clank-transcript` breaks the cycle:

```
clank-shell → clank-core ──────────────────────────────────→ clank-builtins
                    │                                               │
                    └──────────→ clank-transcript ←────────────────┘
```

`clank-transcript` depends only on the standard library. No brush dependency.

### `Transcript` type (`clank-transcript`)

`Transcript` is a bounded sliding window over an ordered sequence of string
entries. It enforces a maximum entry count on every `push`: when the window
is full, the oldest entry is dropped to make room for the new one. This
models the README's "sliding window" property at the structural level, before
any token-budget or summarization machinery is added.

```rust
pub struct Transcript {
    entries: VecDeque<String>,
    max_entries: usize,
}

impl Transcript {
    pub fn new(max_entries: usize) -> Self;
    pub fn push(&mut self, entry: impl Into<String>);  // drops oldest if at capacity
    pub fn clear(&mut self);
    pub fn trim(&mut self, n: usize);    // drop oldest n; if n >= len, clear all
    pub fn entries(&self) -> impl Iterator<Item = &str>; // ordered oldest-first
    pub fn len(&self) -> usize;
    pub fn is_empty(&self) -> bool;
}
```

`VecDeque` is used rather than `Vec` because `push` at capacity requires
`pop_front` — O(1) with `VecDeque`, O(n) with `Vec`. For a sliding window
this matters at any realistic transcript size.

`trim(0)` is a no-op. `trim(n)` where `n >= len()` clears all entries without
error.

The default cap for the shell session is a named constant:
`DEFAULT_MAX_ENTRIES: usize = 1000`. This is a reasonable initial bound; it
is configurable at construction time and can be changed later without API
breakage.

### Process-global `Arc<Mutex<Transcript>>`

`clank-transcript` exposes a module-level function:

```rust
pub fn global() -> Arc<Mutex<Transcript>>
```

This returns a clone of the process-global `Arc`. The global is initialized
on first access via `std::sync::OnceLock` with `DEFAULT_MAX_ENTRIES`. The
`Mutex` is `std::sync::Mutex` (not Tokio's) because the lock is never held
across an await point in any caller.

### Recording in `run_string` wrappers

`clank-core` wraps the `shell.run_string(cmd, &params)` call site in both
`run_with_options` and `run_interactive` with transcript recording:

```
clank_transcript::global().lock().unwrap().push(cmd);
let result = shell.run_string(cmd, &params).await?;
// output recording: deferred (see Out of Scope)
```

At this step, only the command text is recorded. Output capture from
`run_string` is deferred (brush-core does not expose an output interception
hook at the `run_string` level). Recording the command text is sufficient to
make `context show`, `context clear`, and `context trim` observable and
testable.

The recording happens at the same call site in both `run_with_options` (used
by argv mode and script mode) and `run_interactive` (used by interactive
mode). No per-mode instrumentation is required.

### `context` builtin (`clank-builtins`)

A new `ContextBuiltin` type in `clank-builtins` implements
`brush_core::builtins::SimpleCommand`. It is registered with `clank-core` as
part of `default_options()` via `CreateOptions::builtins`.

`context` is registered under the name `"context"` and replaces any
brush-builtins default (there is none; `context` is a clank-only command).

Subcommand dispatch via the first positional argument:

| Invocation | Behaviour |
|---|---|
| `context show` | Acquire lock; write each entry followed by `\n` to stdout; release lock. Does **not** record its own output back into the transcript. |
| `context clear` | Acquire lock; call `transcript.clear()`; release lock. Exit 0. |
| `context trim <n>` | Parse `n` as `usize`; acquire lock; call `transcript.trim(n)`; release lock. Exit 0. Invalid or missing `n` → exit 2, error to stderr. |
| `context` (no subcommand) | Exit 2; usage to stderr. |
| `context <unknown>` | Exit 2; usage to stderr. |

`context show` does not record back: the recording call site wraps
`shell.run_string`, and `context show` itself is executing inside
`run_string`. The recording push happens before `run_string` is called, so
`context show`'s output — written to stdout via `context.stdout()` — never
enters the transcript through the recording path.

`ContextBuiltin` accesses the transcript through `clank_transcript::global()`.

### Manifest registry update (`clank-builtins`)

`context` is added to `MANIFEST_REGISTRY` with scope `ShellInternal`. The
`EXPECTED` table in the unit tests is updated to match.

### `default_options` registration (`clank-core`)

`clank_core::default_options()` gains a `context` builtin registration:

```rust
options.builtins.insert("context".to_owned(), clank_builtins::context_registration());
```

`context_registration()` is a public function in `clank-builtins` that
returns the `Registration` produced by `simple_builtin::<ContextBuiltin>()`.

### Dependency structure

```
clank-shell → clank-core ──────────────────────────────────→ clank-builtins
                    │                                               │
                    └──────────→ clank-transcript ←────────────────┘
                                       (no brush dep)
```

`clank-core` and `clank-builtins` both depend on `clank-transcript`.
`clank-transcript` depends only on the standard library.

`clank-core/Cargo.toml` gains `clank-transcript = { path = "../clank-transcript" }`.
`clank-builtins/Cargo.toml` gains `clank-transcript = { path = "../clank-transcript" }`.
The workspace `Cargo.toml` gains `clank-transcript` as a member.

## Tasks

- [ ] Add `clank-transcript` crate to workspace (`Cargo.toml`,
      `clank-transcript/Cargo.toml`, `clank-transcript/src/lib.rs`)
- [ ] Implement `Transcript` in `clank-transcript`: `VecDeque<String>` backing,
      `max_entries` cap, `DEFAULT_MAX_ENTRIES = 1000`, `push` (drops oldest at
      capacity), `clear`, `trim`, `entries`, `len`, `is_empty`
- [ ] Implement `global() -> Arc<Mutex<Transcript>>` in `clank-transcript`
      using `std::sync::OnceLock`
- [ ] Add unit tests for `Transcript` in `clank-transcript`: push/entries,
      sliding-window eviction at capacity, clear, trim 0, trim within bounds,
      trim exceeding len
- [ ] Add `clank-transcript` as a dependency of `clank-core`
- [ ] Add `clank-transcript` as a dependency of `clank-builtins`
- [ ] Wrap `shell.run_string` call in `run_with_options` to push command text
      to transcript before execution
- [ ] Wrap `shell.run_string` call in `run_interactive` to push command text
      to transcript before execution
- [ ] Add `ContextBuiltin` to `clank-builtins` implementing `SimpleCommand`
      with `show`, `clear`, `trim <n>` dispatch via `clank_transcript::global()`
- [ ] Add `context_registration()` public function to `clank-builtins`
- [ ] Register `context` builtin in `clank_core::default_options()`
- [ ] Add `context` to `MANIFEST_REGISTRY` in `clank-builtins` with scope
      `ShellInternal`; update `EXPECTED` table in unit tests
- [ ] Add acceptance test suite `clank-acceptance/cases/builtins/context.yaml`
      covering all cases listed below
- [ ] Run full test suite; verify no regressions

## Acceptance Tests

All cases are in `clank-acceptance/cases/builtins/context.yaml`.

- `echo hello; context show` → stdout contains `echo hello`, exit 0
- `context clear; context show` → stdout is empty after clear, exit 0
- `echo a; echo b; context trim 1; context show` → only `echo b` (and trim
  and show commands) visible; `echo a` absent
- `context trim 0; context show` → transcript unchanged (trim 0 is no-op)
- `echo a; echo b; context trim 999; context show` → stdout empty (trim
  exceeding count clears all)
- `context show` output does not appear inside itself on re-inspection
  (no duplication: run `context show` twice; second show does not contain
  first show's output as a transcript entry)
- `context trim notanumber` → exit 2, stderr non-empty
- `context unknowncmd` → exit 2, stderr non-empty
- `context` (no subcommand) → exit 2, stderr non-empty

The sliding-window eviction property is covered by unit tests in
`clank-transcript` rather than acceptance tests; verifying it through the
acceptance harness would require filling the window to capacity (1000
entries), which is impractical in a shell script.

## Out of Scope

- Output capture from command execution — brush-core 0.4.0 exposes no output
  interception hook at the `run_string` level. Recording command text only is
  sufficient for this step.
- `context summarize` — requires model access; separate issue.
- Token-budget compaction (summarize-and-replace at the leading edge) — separate issue; requires model access. The entry-count cap in `Transcript` is the structural foundation for this future work.
- Redaction rules applied to transcript entries — separate issue.
- Golem oplog integration and durability — separate issue.
- `ask` consuming the transcript as context — depends on this issue; separate.
