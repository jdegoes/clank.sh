---
title: "No transcript data structure or context builtin — transcript exists only conceptually"
date: 2026-03-07
author: agent
---

# No transcript data structure or context builtin — transcript exists only conceptually

## Problem

The shell currently has no transcript. The README defines the transcript as
a first-class shell-owned value — the sliding-window record of everything
rendered to the terminal — and names it as the central architectural choice
underpinning the entire AI integration story. At present, nothing in the
codebase represents, stores, or manages it.

Without a transcript, the following are impossible:

- `context show` — there is nothing to print
- `context clear` — there is nothing to discard
- `context trim <n>` — there are no entries to drop
- `ask` — the model has no context window to receive

The `context` builtin is classified as `shell-internal` in both the README
architecture diagram and the `ExecutionScope` type in `clank-builtins`. The
manifest registry already declares `context` as `ShellInternal` — but no
such command exists, and no transcript backing it exists.

## Capability Gap

Two distinct missing pieces must be addressed together:

**1. Transcript data structure (`clank-core`)**

A type representing the transcript as an ordered sequence of entries. Each
entry is one unit of terminal-rendered content: a command line typed by the
user, a block of output from a command, or a response from the AI. The
structure must support:

- Appending entries
- Clearing all entries
- Trimming the oldest `n` entries
- Iterating entries for display (`context show`)

This is the minimal surface needed for `context show`, `context clear`, and
`context trim`. Summarization (`context summarize`) is explicitly deferred
and is not part of this issue.

The transcript must be owned by the shell and passed into the builtin
implementation — it is not a global or a side-channel.

Recording must happen at a single, run-mode-agnostic call site. All three
execution modes in `clank-core` — argv (`run`/`run_with_options`), script
(same path), and interactive (`run_interactive`) — converge on
`shell.run_string(...)`. That is the correct and only place to record
entries: before the call (to capture the command text) and after (to capture
output). The transcript design must not require callers to instrument
recording independently, and must not differ by run mode.

**2. `context` builtin (`clank-builtins`)**

A `shell-internal` builtin registered with `brush-core` via its extension
API. Subcommands in scope for this issue:

| Subcommand | Behaviour |
|---|---|
| `context show` | Print all transcript entries to stdout. Does **not** record its own output back into the transcript. |
| `context clear` | Discard all transcript entries. The next `ask` invocation will see an empty context. |
| `context trim <n>` | Drop the oldest `n` entries. `n` must be a non-negative integer; `trim 0` is a no-op. |

`context summarize` is explicitly out of scope — it requires model access
and is deferred to a later issue.

## Why Both Together

`context` is useless without a transcript; a transcript is invisible without
`context show`. The acceptance test surface for the transcript feature is
`context show`, `context clear`, and `context trim` — without the builtin,
the transcript cannot be verified through the acceptance harness. The two
pieces are coupled and must ship together.

## Acceptance Surface

The acceptance harness (`clank-acceptance`) spawns the compiled `clank`
binary and asserts on stdout, stderr, and exit code. The `context` subcommands
are the observable interface through which transcript state can be verified:

- Run a command that produces output, then `context show` — output should
  contain the transcript entry for that command.
- `context clear` followed by `context show` — output should be empty (or
  show only the clear operation itself, depending on how recording is
  sequenced).
- `context trim <n>` followed by `context show` — the oldest `n` entries
  should be absent.
- `context trim 0` — no-op; transcript unchanged.
- `context trim <n>` where `n` exceeds entry count — all entries dropped,
  no error.
- `context show` output must **not** duplicate itself when its own output
  reaches the terminal (the README requirement that inspection commands do
  not record back).

## Out of Scope

- `context summarize` — requires model access; deferred.
- Sliding-window compaction and token-budget enforcement — deferred.
- Redaction rules applied to transcript entries — deferred.
- Golem oplog integration and durability — deferred.
- `ask` consuming the transcript as context — depends on this issue but is
  a separate effort.
