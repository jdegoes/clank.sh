---
title: "Transcript records commands only — not their output, timestamp, or kind"
date: 2026-03-07
author: agent
---

# Transcript records commands only — not their output, timestamp, or kind

## Problem

The current `Transcript` type stores `VecDeque<String>` — one plain string per
entry, containing only the command text that was executed. This satisfies
`context show`, `context clear`, and `context trim`, but it is structurally
unable to represent the artefact the README defines:

> The shell's sliding-window record of everything rendered to the terminal in
> the current session: **every command typed, every output produced, every AI
> response**.

The current implementation is a command history. The spec requires a
session transcript: a time-ordered sequence of typed commands interleaved with
their output, which together form the full picture of what happened in the
session. This is the value that `ask` will receive as its context window.

## Specific Gaps

**1. No output capture.** When `ls` runs, the listing is rendered to the
terminal but never enters the transcript. When `ask` eventually reads the
transcript, it will see `ls` was invoked but not what it returned. The model
cannot reason about command results.

**2. No timestamps.** The spec's sliding window is bounded by token budget,
not raw entry count. A model needs timestamps to reason about recency.
`context trim <n>` and future compaction machinery also benefit from knowing
when entries were created.

**3. No entry kind.** The spec distinguishes: command text (typed by the user
or AI), command output, and AI responses. A flat `String` cannot carry this
distinction. Without it, `context show` cannot format the transcript
meaningfully, and `ask` cannot construct a well-formed conversation history
for the model (e.g. differentiating user turns from assistant turns from tool
output).

**4. No `context show` / `context summarize` output re-entry guarantee at the
entry level.** The current implementation achieves the "output not recorded
back" guarantee structurally — stdout never flows through the recording call
site. But with a richer entry type, this guarantee must remain explicit and
tested, especially once output capture is added.

## Required Change

`TranscriptEntry` must become a typed, timestamped value. Timestamps use
`chrono::DateTime<Utc>` rather than `std::time::SystemTime`: chrono provides
ergonomic formatting (`to_rfc3339()`), arithmetic, and serde integration that
`SystemTime` lacks.

`chrono` is safe to add as a dependency for `wasm32-wasip2`: WASI targets
use `std::time::SystemTime` (via the WASI clock API) unconditionally. The
`wasmbind` feature and wasm-bindgen/js-sys are not needed and will not be
pulled in. The problematic `Utc::now()` panic only affects
`wasm32-unknown-unknown` (browser WASM without wasmbind), not WASI targets.

```rust
pub struct TranscriptEntry {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub kind: EntryKind,
}

pub enum EntryKind {
    Command(String),          // text typed or executed
    Output(String),           // stdout/stderr from a command
    AiResponse(String),       // response from the model via ask
}
```

`Transcript` holds `VecDeque<TranscriptEntry>`. The sliding-window eviction
and all existing operations (`push`, `clear`, `trim`, `entries`, `len`,
`is_empty`) are updated accordingly.

Output capture requires a mechanism to intercept what brush-core writes to
the process's stdout/stderr. The current `run_string` API does not expose an
output hook. This is a known constraint documented in the realized design
(`dev-docs/designs/proposed/transcript-and-context-builtin.md`); this issue
requires resolving it as part of the implementation.

## Interaction with `context show` / `context summarize`

The README states:

> `context show` and `context summarize` are transcript-inspection commands:
> their output is written to stdout but is **not recorded back into the
> transcript**.

The word "back" is deliberate. The commands themselves are recorded as entries
(the human or AI invoked them — that is part of the session history). Their
*output* — the rendered transcript contents written to stdout — must not
re-enter as new entries.

With output capture in place, this requires an explicit exclusion: when the
shell intercepts output, it must not record stdout produced by `context show`
or `context summarize`. The mechanism for this exclusion is a design decision
for the plan.

## Out of Scope

- `context summarize` (requires model access)
- Token-budget compaction (requires model access)
- `ask` consuming the transcript as a context window (depends on this issue;
  separate)
- AI response recording (requires `ask` to feed responses back; depends on
  `ask` integration)
- Redaction rules
- Golem oplog integration
