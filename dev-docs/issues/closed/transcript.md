---
title: No transcript — the shell has no memory of what happened
date: 2026-03-07
author: agent
---

## Summary

The transcript is the central architectural feature of clank.sh. Without it,
the shell is a Bash wrapper. With it, clank.sh becomes what the README
describes: a shell where the AI reads exactly what the human sees.

From the README:

> "The shell maintains a sliding-window transcript of everything rendered to
> the terminal: every command typed, every output produced, every AI response.
> This is what `ask` operates on. It is not a separate AI context that must be
> populated — it is the shell's own history, extended to include AI exchanges
> and treated as a first-class value."

## Problem

The current shell has no transcript. Commands run, output goes to stdout, and
it is gone. Nothing is recorded. The shell has no memory of what happened.

The consequences:

- `ask` cannot be implemented — it has no context to send to the model
- The central design principle ("The AI reads exactly what you see") is false
- The `context` builtin cannot exist
- Every command run so far contributes nothing to AI awareness

## What the Transcript Must Do

Per the README:

1. **Record everything rendered to the terminal** — every command typed, every
   output produced, every AI response, in order.

2. **Be a sliding window** — when it approaches the token budget, the oldest
   portion is compacted: summarized and replaced with a visible summary block.
   The boundary is always explicit.

3. **Be a first-class shell value** — owned by the shell, not the terminal.
   `context clear` is an operation on the shell's own data.

4. **Be readable by `ask`** — `ask` receives the current transcript as its
   context window on every invocation. No setup, no curation.

5. **Respect redaction** — anything governed by `redaction-rules` in a command
   manifest never enters the transcript.

## The `context` Builtin

The README defines `context` as the management interface for the transcript.
It is `shell-internal` scoped (operates on shell-internal tables):

```
context show          # print current transcript to stdout
context clear         # discard transcript (AI starts fresh on next ask)
context summarize     # print a summary of the current transcript to stdout
context trim <n>      # drop oldest n entries from transcript
```

`context show` and `context summarize` write to stdout but do NOT record back
into the transcript — this prevents infinite self-duplication.

## Scope of This Issue

This issue covers the foundational transcript layer:

1. **The transcript data structure** — an in-memory, ordered record of shell
   I/O entries owned by the shell.
2. **Recording** — every command input and its output is appended to the
   transcript as the REPL runs.
3. **The `context` builtin** — `show`, `clear`, `trim`. `summarize` is deferred
   (requires `ask` / HTTP, which is a subsequent issue).
4. **`ask` read access** — a public API on the shell that returns the current
   transcript as a string, ready to be passed to a model.

Sliding-window compaction and redaction rules are deferred — they depend on
`ask` and the command manifest system respectively, neither of which exists yet.

## Why This Must Be Solved Before `ask`

`ask` requires:
1. Read the current transcript — **this issue**
2. Make an HTTP call to a model provider — subsequent issue
3. Append the model response back to the transcript — **this issue**

Steps 1 and 3 are impossible without this issue resolved.

## Acceptance Condition

- Every command typed and its output is recorded in the transcript during the
  REPL loop.
- `context show` prints the full transcript to stdout.
- `context clear` empties the transcript.
- `context trim <n>` drops the oldest n entries.
- A public `transcript()` method on the shell returns the current transcript
  as a `String`, ready for use by `ask`.
- All existing tests continue to pass.
