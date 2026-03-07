---
title: Transcript grows unboundedly — no token budget or sliding-window compaction
date: 2026-03-07
author: agent
---

## Problem

The current `Transcript` implementation has no token budget. In a long shell
session, the transcript accumulates every command and every output indefinitely.
When `ask` is implemented, it will pass the full transcript as context to the
AI model. Without compaction, the transcript will eventually exceed the model's
context window, causing `ask` to fail or silently truncate the context.

From the README:

> "When the transcript approaches the token budget, the oldest portion is
> automatically summarized and replaced with a visible summary block. The
> boundary is always explicit."

## Capability Gap

`Transcript` has `clear()` and `trim()` but no automatic compaction. There is
no token budget, no approximation of token count, and no `Summary` entry kind.
The `context summarize` subcommand is also missing.

## What Is Required

1. **Token budget** — `Transcript::new(max_tokens)` with a sensible default
   (e.g. 8,000 tokens).
2. **Token count approximation** — `text.len() / 4` is a widely-used
   approximation (1 token ≈ 4 characters). Accurate enough for planning the
   window; actual limits are enforced by the provider.
3. **`Summary` entry kind** — a special entry that represents compacted
   history. Never appended directly — produced by `compact()`.
4. **Automatic compaction** — when `push_command` or `push_output` causes
   `token_count()` to exceed `max_tokens`, `compact()` fires automatically,
   replacing the leading entries with a `Summary` entry. At least the most
   recent entry is always preserved.
5. **`render()` with semantic labels** — entries rendered with `[input]`,
   `[output]`, `[error]`, `[ai]`, `[summary]` prefixes so the model can
   distinguish entry kinds without ambiguity.

## Acceptance Condition

- A transcript with a 100-token budget that receives 101 tokens of entries
  automatically compacts, replacing leading entries with a `Summary` entry.
- The most recent entry is always preserved after compaction.
- `render()` produces `[input] echo hello\n[output] hello\n` format.
- All existing tests continue to pass.
