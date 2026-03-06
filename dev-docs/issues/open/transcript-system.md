---
title: "No transcript system — shell has no session history or AI context window"
date: 2026-03-06
author: agent
---

## Problem

The shell currently has no transcript. Every command runs and produces output, but nothing is
recorded. The session has no memory of what has happened, and there is no structure that could
serve as the AI's context window.

The transcript is the central architectural idea of clank.sh: the shell's session history *is*
the model's context window. Without it, `ask`, `context`, and all AI integration are impossible.
The transcript is also required for the `context` builtin, sliding-window compaction, and any
future features that reason about session history.

## Desired Outcome

A `Transcript` type owned by `clank-core` that:

- Records every entry appended to the session: user input lines, command output, and (eventually)
  AI responses.
- Maintains a sliding window bounded by a configurable token budget, compacting the leading edge
  by replacing it with a summary block when the budget is approached.
- Exposes a read view suitable for passing to a model provider as context.
- Is accessible from the REPL loop so every command and its output is recorded automatically.
- Is the single source of truth for `context show`, `context clear`, `context trim`, and
  `context summarize` (builtins to be implemented in a subsequent task).
