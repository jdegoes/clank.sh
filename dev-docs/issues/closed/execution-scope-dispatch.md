---
title: "No execution scope dispatch — all commands treated identically"
date: 2026-03-06
author: agent
---

# No execution scope dispatch — all commands treated identically

## Problem

The shell currently has no concept of execution scope. Every command string is passed directly to `brush-core`'s `Shell::run_string`, which handles it using its own internal logic. There is no clank-level mechanism to classify a command by scope, route it to the correct execution path, or enforce the invariants each scope requires.

The README defines three execution scopes that are fundamental to clank's architecture:

| Scope | Meaning |
|---|---|
| `parent-shell` | Runs in the parent shell's context; may mutate shell state (cwd, env, function table). POSIX special builtins. Cannot be overridden or run as a subprocess. |
| `shell-internal` | Implemented entirely in the shell; operates on internal tables (alias table, job table, transcript, history). Cannot run as a subprocess. |
| `subprocess` | Isolated execution; no access to parent shell state. Scripts, prompts, Golem agents, installed executables. |

Without this dispatch layer, clank cannot:

- Implement `cd` correctly (`parent-shell`: mutates cwd on the parent shell, not a subprocess)
- Implement `alias` correctly (`shell-internal`: reads and writes the shell's alias table)
- Distinguish `ls` from `cd` at the clank layer (`subprocess` vs `parent-shell`)
- Enforce that `parent-shell` and `shell-internal` commands cannot be invoked as subprocesses
- Expose the correct command surface to `ask` (only `subprocess`-scoped commands are AI tools)
- Associate an `authorization-policy` with a command (policy lives on the manifest, which requires scope classification)

## Impact

Every subsequent feature that involves command dispatch — builtins, transcript integration, AI tool surface, authorization — depends on execution scope being defined and dispatched correctly. Without it:

- `cd` cannot be built: a subprocess `cd` has no effect on the parent shell's working directory.
- `alias` cannot be built: the alias table does not exist as a clank-owned structure.
- `ls` as a subprocess cannot be distinguished from builtins at the clank layer.
- The AI tool surface cannot be bounded correctly — `ask` must only see `subprocess`-scoped commands.

## Context

`brush-core` has its own notion of "special builtins" vs ordinary builtins vs external commands, but this is internal to the interpreter and does not map directly to clank's three-scope model. In particular:

- `brush-core` does implement `cd` as a builtin that mutates the shell's working directory correctly. Until clank overrides it, the inherited behavior works. But clank needs to own this classification to enforce its manifest model, authorization policy, and AI tool-surface filtering.
- `brush-core` does implement `alias`. Similarly, the inherited behavior works until clank needs to own and expose the alias table as a clank-managed structure.
- `brush-core` dispatches external commands as OS subprocesses. In clank, `subprocess`-scoped commands must eventually be dispatched via clank's internal process trait (not OS fork/exec), but this is a future concern blocked by the process model work. For now, OS subprocess dispatch is the correct fallback.

The immediate need is to establish the **structural foundation**: a Rust type representing execution scope, the command manifest skeleton, and the routing logic that consults it. The three example commands — `cd` (`parent-shell`), `alias` (`shell-internal`), `ls` (`subprocess`) — are the acceptance surface for this foundation.

## Out of Scope

- Full command manifest implementation (authorization-policy, redaction-rules, help-text, subcommand trees) — that is a separate, larger effort.
- Full process table and PID tracking — separate effort.
- Job control (`fg`, `bg`, `jobs`) — separate effort.
- AI tool surface filtering (`ask` only sees `subprocess`-scoped commands) — depends on this issue but is a separate effort.
- Replacing OS subprocess dispatch with clank's internal process trait — blocked by process model work; separate effort.
