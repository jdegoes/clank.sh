---
title: "Execution Scope Dispatch Foundation"
date: 2026-03-06
author: agent
issue: "dev-docs/issues/open/execution-scope-dispatch.md"
research: []
designs: []
---

# Execution Scope Dispatch Foundation

## Originating Issue

No execution scope dispatch — all commands treated identically. See `dev-docs/issues/open/execution-scope-dispatch.md`.

## Research Consulted

No formal research documents written. The relevant facts were established by direct inspection of the `brush-core` and `brush-builtins` source.

**`brush-builtins` already implements `cd` and `alias` correctly:**
- `cd` (`brush-builtins/src/cd.rs`) calls `context.shell.set_working_dir()` — correct `parent-shell` behavior; it mutates the shell's working directory via `&mut Shell`.
- `alias` (`brush-builtins/src/alias.rs`) reads and writes `context.shell.aliases` — correct `shell-internal` behavior; it operates on the shell's alias table.
- Both are registered as non-special builtins in `brush-builtins/src/factory.rs` with no execution-scope classification.

**`brush-core` builtin registration API:**
- `Shell::register_builtin(name, Registration)` registers a builtin at runtime.
- `Registration` has `special_builtin: bool` but no concept of clank's three-scope model.
- `brush-builtins::builtin::<T>()` and `simple_builtin::<T>()` produce `Registration` values from types implementing `builtins::Command`.

**`ls` is an external command** — dispatched by `brush-core` as an OS subprocess. No builtin registration involved.

## Developer Feedback

Three design questions were posed and answered:

1. **Builtin implementation strategy:** Inherit brush-builtins' `cd` and `alias` implementations; layer execution scope metadata on top via a clank-owned manifest registry. No reimplementation for this step.

2. **New workspace crate:** Approved. Add `clank-builtins` as a new workspace crate. It owns `ExecutionScope`, `CommandManifest`, the manifest registry, and all future clank-owned builtin implementations. It depends on `brush-core` (already in the workspace). `clank-core` depends on `clank-builtins`.

3. **Misleading code comment:** The doc comment in `clank-core/src/lib.rs` that states "All direct `brush_core` imports live here. No other crate in the workspace depends on `brush_core` directly." is now wrong. It must be removed as part of this work.

## Approach

### New crate: `clank-builtins`

A new `clank-builtins` workspace crate is added. It owns:

- `ExecutionScope` — the three-value enum (`ParentShell`, `ShellInternal`, `Subprocess`)
- `CommandManifest` — a struct carrying `name: &'static str` and `scope: ExecutionScope` (minimal for this step; extended in future work)
- `MANIFEST_REGISTRY` — a static slice of `CommandManifest` entries for all commands clank classifies. For this step: `cd`, `alias`, `ls`.
- A public function `scope_of(name: &str) -> Option<ExecutionScope>` that looks up a command name in the registry.

No builtin *implementations* in `clank-builtins` for this step — `cd` and `alias` continue to come from `brush-builtins`. The crate is pure metadata for now.

### Corrected `clank-core/src/lib.rs` comment

The module-level doc comment claiming `brush_core` imports are exclusive to `clank-core` is removed. No replacement comment — the code is self-explanatory.

### `clank-core` wires scope-awareness into `run_with_options`

`clank-core` gains a dependency on `clank-builtins`. No behavioral change is needed for `cd`, `alias`, or `ls` at this step — `brush-builtins` handles `cd` and `alias` correctly, and `ls` runs as an OS subprocess via `brush-core`'s existing dispatch. The wiring for this step is:

- `scope_of` is callable from `clank-core`
- The manifest registry is populated with the three example commands

The routing enforcement (blocking `parent-shell` commands from subprocess context, filtering AI tool surface) is future work. The foundation — the types and registry — must exist before any of that can be built.

### Acceptance surface

Three commands must work correctly in the interactive REPL after this change:

- `cd /tmp` — changes the working directory (verifiable with `pwd`)
- `alias ll='ls -l'` followed by `ll` — defines and invokes an alias
- `ls` — lists directory contents as a subprocess

These work today via `brush-builtins` inheritance. The acceptance tests added in this step verify they continue to work and that clank's manifest registry correctly classifies each one.

### `clank-builtins` unit tests

- `scope_of("cd")` returns `Some(ExecutionScope::ParentShell)`
- `scope_of("alias")` returns `Some(ExecutionScope::ShellInternal)`
- `scope_of("ls")` returns `Some(ExecutionScope::Subprocess)`
- `scope_of("unknown-command")` returns `None`

### Acceptance tests

New YAML cases under `clank-acceptance/cases/builtins/`:

- `cd` changes working directory: `cd /tmp && pwd` → stdout contains `/tmp`
- `alias` defines and expands: `alias hi='echo hello' && hi` → stdout contains `hello`
- `ls` runs as subprocess: `ls /tmp` → exit 0 (stderr empty is not asserted; `/tmp` may have restricted output on some systems)

## Tasks

- [ ] Add `clank-builtins` crate to workspace (`Cargo.toml`, `clank-builtins/Cargo.toml`, `clank-builtins/src/lib.rs`)
- [ ] Define `ExecutionScope` enum in `clank-builtins`
- [ ] Define `CommandManifest` struct in `clank-builtins`
- [ ] Populate `MANIFEST_REGISTRY` with entries for `cd`, `alias`, `ls`
- [ ] Implement `scope_of(name: &str) -> Option<ExecutionScope>` in `clank-builtins`
- [ ] Add unit tests for `scope_of` in `clank-builtins`
- [ ] Add `clank-builtins` as a dependency of `clank-core`
- [ ] Remove the now-incorrect exclusivity comment from `clank-core/src/lib.rs`
- [ ] Add acceptance test cases for `cd`, `alias`, `ls`
- [ ] Run full test suite; verify no regressions

## Acceptance Tests

- `cd /tmp && pwd` → stdout contains `/tmp`, exit 0
- `alias hi='echo hello' && hi` → stdout contains `hello`, exit 0
- `ls /tmp` → exit 0
- `scope_of("cd")` → `Some(ParentShell)`, `scope_of("alias")` → `Some(ShellInternal)`, `scope_of("ls")` → `Some(Subprocess)`, `scope_of("unknown")` → `None`
