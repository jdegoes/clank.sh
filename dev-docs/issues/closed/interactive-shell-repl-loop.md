---
title: "Shell does not operate as an interactive REPL"
date: 2026-03-06
author: agent
---

# Shell does not operate as an interactive REPL

## Problem

The `clank` binary currently accepts a single command from argv or stdin, executes it once, and exits. This is a script-execution model, not an interactive shell. There is no read-eval-print loop, no prompt, and no way for a user to enter successive commands in a session.

## Impact

Without a REPL loop, the shell cannot be used interactively. All subsequent shell features — execution scope, job control, transcript management, `ask` invocation — require a live session in which the user can issue multiple commands over time. The current design blocks all interactive development and testing of the shell's command surface.

## Context

`clank-shell/src/main.rs` delegates to `clank_core::run(&command)`, which boots a `brush_core::Shell`, executes the single command string, and returns. The shell process then terminates. There is no loop, no prompt emission, and no stdin-read cycle.

`brush-core` exposes `Shell::run_string` for executing individual command strings against a persistent shell instance. A REPL loop can be built on top of this: create the shell once, then repeatedly read a line from stdin, execute it, print output, and continue until EOF or `exit`.

The initial interactive loop does not need to implement readline editing, history, or tab completion — those are separate concerns. A bare `read line → run_string → repeat` cycle is the minimal correct behavior.

## Out of Scope

- Readline / line-editing (arrow keys, history recall, tab completion)
- `PS1` / `PS2` prompt customization
- Job control (`&`, `jobs`, `fg`, `bg`) — separate feature
- Transcript management — separate feature
- Execution scope dispatch (`cd`, `alias`, `ls`)
