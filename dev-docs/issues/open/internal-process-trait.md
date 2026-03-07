---
title: No internal process trait — shell dispatches to real OS processes
date: 2026-03-07
author: agent
---

## Problem

The current implementation delegates all command execution to `brush-core`, which
resolves unknown commands by spawning real OS child processes via `fork`/`exec` (using
the `nix` crate). This is correct behaviour for a traditional Unix shell, but it is
fundamentally incompatible with clank.sh's architecture and goals.

clank.sh is explicitly **not** a Unix process kernel. From the README:

> "clank.sh is a single WebAssembly component, not a traditional Unix shell with real
> OS processes. Everything that looks like a process is a synthetic abstraction inside
> that component: builtins, scripts, prompts, and Golem agent invocations are all
> distinct implementations of the same internal process trait. There is no fork, no
> exec, no Unix signal kernel."

The consequence of the current approach:

1. **WASM incompatibility.** `fork`/`exec` and the `nix` crate do not exist in
   `wasm32-wasip2`. The current code cannot compile to WASM and therefore cannot be
   deployed to Golem.

2. **No sandbox.** Any command on the host `$PATH` can be invoked. The AI can reach
   outside what is explicitly installed. The security model described in the README
   does not exist.

3. **No authorization.** Command manifests, `allow`/`confirm`/`sudo-only` policies,
   and the `P` process state cannot be enforced without controlling the dispatch layer.

4. **No synthetic process table.** PIDs, process states (`R`, `S`, `T`, `Z`, `P`),
   `ps`, `/proc/`, and job control (`&`, `jobs`, `fg`, `bg`) cannot be implemented
   without owning the process abstraction.

5. **No transcript integration.** stdout/stderr from commands must flow through
   clank's transcript layer. This is impossible if commands are real OS processes
   writing directly to inherited file descriptors.

## Capability Gap

There is no internal async process trait. There is no process table. There is no
command dispatch layer that clank.sh owns and controls. Brush's OS process spawning
is the only execution path for any command not registered as a `brush-core` builtin.

## What the README Requires

From the Architecture section:

> "Everything that appears to the user as a 'process' is an abstraction internal to
> that component, modeled by an async Rust trait. Different process types — builtins,
> scripts, prompts, Golem agent invocations — are distinct implementations of that
> trait, not separate WASM components."

From the Process Model section, three execution scopes must be supported:

| Scope | Examples |
|---|---|
| `parent-shell` | `cd`, `exec`, `exit`, `export`, `source`, `unset` |
| `shell-internal` | `alias`, `context`, `jobs`, `prompt-user`, `read`, `type`, `wait` |
| `subprocess` | `ls`, `grep`, `echo`, `ask`, scripts, prompts, agent executables |

All three scopes must be implemented as internal async Rust — none should ever invoke
a real OS process.

## Why This Must Be Solved Next

Every subsequent feature depends on this:

- Core commands (`ls`, `cat`, `grep` etc.) cannot be implemented without it
- The transcript cannot intercept command output without it
- `ask` cannot be implemented without it
- Authorization cannot be enforced without it
- WASM compilation is blocked by `nix` usage in the current path
- The sandbox does not exist without it

This is the foundational architectural layer. Nothing above it can be built correctly
until it exists.

## Acceptance Condition

A developer can run `echo hello` and `true` and `false` through clank's internal
dispatch layer — without spawning any OS process. The process trait exists, at least
one `subprocess`-scoped command is implemented against it, and Brush's OS process
spawning path is no longer reachable for registered commands.
