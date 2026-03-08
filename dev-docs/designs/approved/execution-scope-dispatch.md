---
title: "Execution Scope Dispatch Foundation"
date: 2026-03-06
author: agent
---

# Execution Scope Dispatch Foundation

## Overview

This document describes the execution scope dispatch foundation as realized in the codebase. It supersedes the plan `dev-docs/plans/approved/execution-scope-dispatch.md` as the reference for future work in this area.

## New crate: `clank-builtins`

A `clank-builtins` workspace crate was added. It owns execution scope metadata and will house future clank-owned builtin implementations. It depends directly on `brush-core` and `brush-builtins`.

`clank-core` depends on `clank-builtins`. `clank-shell` continues to depend only on `clank-core`.

## Types

### `ExecutionScope`

```rust
pub enum ExecutionScope {
    ParentShell,    // mutates shell state; cannot run as subprocess
    ShellInternal,  // operates on shell-owned tables; cannot run as subprocess
    Subprocess,     // isolated; no parent shell state access; AI tool surface
}
```

Derives `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`.

### `CommandManifest`

```rust
pub struct CommandManifest {
    pub name: &'static str,
    pub scope: ExecutionScope,
}
```

Minimal for this step. Future fields: `synopsis`, `authorization_policy`, `input_schema`, `help_text`, `subcommands`.

## Registry

`MANIFEST_REGISTRY: &[CommandManifest]` is a static slice. Entries are grouped by scope for readability; the registry is sorted alphabetically within each group.

| Command | Scope |
|---|---|
| `.`, `cd`, `exec`, `exit`, `export`, `source`, `unset` | `ParentShell` |
| `alias`, `bg`, `fg`, `history`, `jobs`, `read`, `type`, `unalias`, `wait` | `ShellInternal` |
| `ask`, `cat`, `curl`, `find`, `grep`, `ls` | `Subprocess` |

## Public API

```rust
pub fn scope_of(name: &str) -> Option<ExecutionScope>
```

Linear scan of `MANIFEST_REGISTRY`. Returns `None` for commands not yet classified by clank; callers fall through to brush-core's default dispatch. As the registry grows, a hash map can replace the linear scan without changing the API.

## What is not yet enforced

The registry is **metadata only** at this step. No routing enforcement exists yet:

- `parent-shell` commands are not blocked from subprocess invocation
- `shell-internal` commands are not blocked from subprocess invocation
- `ask`'s tool surface is not yet filtered to `Subprocess`-scoped commands only

These are downstream consumers of `scope_of` and are separate future work items.

## Behavioral correctness of example commands

`cd` and `alias` work correctly because `brush-builtins` implementations mutate `context.shell` directly via `&mut Shell` — the same mechanism clank relies on. `ls` dispatches as an OS subprocess via brush-core's existing external command path. No clank-level routing change was required for correctness at this step.

## Test coverage

### Unit tests (`clank-builtins/src/lib.rs`)

Two tests:

- `registry_matches_expected` — sorted snapshot comparison between `MANIFEST_REGISTRY` and a parallel `EXPECTED` constant in the test module. Verifies every command name and scope. Order-independent: both sides are sorted by name before diffing, so entries can be grouped freely in the registry without breaking the test.
- `unknown_command_returns_none` — `scope_of("unknown-command") == None`

When adding a command, one line goes in `MANIFEST_REGISTRY` and the matching line goes in `EXPECTED`. The sorted diff in the failure output makes any mismatch immediately legible.

### Acceptance tests (`clank-acceptance/cases/builtins/`)

One YAML file per command, named after the command. Files are independent — finding tests for a command means opening `<command>.yaml`.

| File | Cases |
|---|---|
| `alias.yaml` | define and invoke, with argument, list all, show specific |
| `break.yaml` | exits loop early |
| `cd.yaml` | changes directory, no-arg goes home, dash returns to previous |
| `continue.yaml` | skips current iteration |
| `declare.yaml` | integer arithmetic |
| `dot.yaml` | sources file into current shell |
| `echo.yaml` | prints string, no-newline flag |
| `eval.yaml` | executes string, constructs command dynamically |
| `exit.yaml` | specific exit code, exit zero |
| `export.yaml` | makes variable visible |
| `false.yaml` | exits nonzero |
| `getopts.yaml` | parses short flag |
| `hash.yaml` | caches command path |
| `local.yaml` | scopes variable to function |
| `ls.yaml` | lists directory |
| `pwd.yaml` | prints working directory, reflects cd |
| `read.yaml` | reads single var, reads multiple vars |
| `return.yaml` | sets exit code, exits function early |
| `set.yaml` | sets positional parameters |
| `shift.yaml` | shifts positional params in function |
| `test.yaml` | numeric equality, bracket form, directory existence, string inequality |
| `true.yaml` | exits zero |
| `type.yaml` | identifies builtin, identifies alias |
| `unalias.yaml` | removes alias |
| `unset.yaml` | removes variable, removes function |

## Deviations from Plan

Two deviations from the approved plan:

**Registry test approach changed.** The plan specified per-command test functions (`cd_is_parent_shell`, etc.). The implementation uses a sorted snapshot comparison (`registry_matches_expected`) against a parallel `EXPECTED` constant, which covers the entire registry in one test and scales without per-command boilerplate.

**Acceptance test structure changed.** The plan specified cases under `cases/builtins/execution-scope.yaml`. The implementation uses one file per command named after the command, covering a broader set of builtins. The scope-based grouping is an internal architectural concept not exposed in the test file names.
