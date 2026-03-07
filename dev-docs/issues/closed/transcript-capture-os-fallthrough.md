---
title: "Transcript does not capture output from OS-fallthrough commands"
date: 2026-03-06
author: agent
---

# Transcript does not capture output from OS-fallthrough commands

## Problem

The session transcript only captures output from commands that are routed through clank's
internal dispatch table. Commands that fall through to Brush's OS process-spawning path —
full-path invocations like `/bin/ls`, host commands not in the registry, or any command
Brush resolves via `$PATH` rather than our builtin table — execute and produce visible
output, but that output is never written to the transcript.

This breaks the core property of the shell: *the model sees exactly what the human sees*.
When the user runs `/bin/ls /tmp` and then `ask "what did ls show?"`, the model has no
context and cannot answer.

## Root cause

Our tempfile-based stdout capture in `ClankShell::run_line()` only redirects stdout for
commands dispatched through the internal process table. Brush's native OS process spawning
(triggered by full-path commands or `$PATH` resolution fallthrough) writes directly to the
terminal, bypassing the capture entirely.

## Impact

- High: the transcript's core usefulness is compromised for any session involving real OS
  commands.
- Workaround: none currently. Users must avoid full-path invocations and wait for Phase 2.

## Resolution

Addressed in Phase 2 when the full process model is built. The correct fix is to intercept
output at the Brush execution layer for all command types, not just dispatch-table commands.
This requires the virtual process table and proper I/O wiring across all code paths —
both of which are Phase 2 deliverables.
