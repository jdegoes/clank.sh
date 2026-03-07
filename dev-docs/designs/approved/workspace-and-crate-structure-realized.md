---
title: "Workspace and Crate Structure (Realized)"
date: 2026-03-06
author: agent
realized_design: true
supersedes: "dev-docs/designs/proposed/workspace-and-crate-structure.md"
---

# Workspace and Crate Structure — Realized Design

## Overview

This document records the workspace and crate structure as actually implemented after Phase -1.
It supersedes the proposed design (`workspace-and-crate-structure.md`) as the reference for
future plans. The proposed design remains as permanent record of intent.

Deviations from the proposed design are noted inline.

---

## Workspace Layout (as built)

```
clank.sh/
├── Cargo.toml              # workspace root; workspace.dependencies for all shared deps
├── Cargo.lock
├── rust-toolchain.toml     # pins stable Rust toolchain
├── AGENTS.md
├── crates/
│   ├── clank/              # binary crate; main.rs only; wires DI and starts shell
│   │   ├── src/main.rs
│   │   └── tests/
│   │       └── shell_basics.rs   # system tests (assert_cmd)
│   ├── clank-shell/        # Brush integration, Process trait, dispatch, interactive loop
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── builtins.rs       # global dispatch table, Brush Registration map
│   │       ├── extensions.rs     # ClankExtensions placeholder
│   │       ├── process.rs        # Process trait, ProcessContext, ProcessIo, StubProcess
│   │       └── shell.rs          # ClankShell, run_line, run_interactive
│   ├── clank-http/         # HttpClient trait, NativeHttpClient, MockHttpClient
│   │   └── src/lib.rs
│   ├── clank-vfs/          # Vfs trait, RealFs, MockVfs
│   │   └── src/lib.rs
│   ├── clank-ask/          # stub only
│   │   └── src/lib.rs
│   ├── clank-manifest/     # CommandManifest, ExecutionScope, AuthorizationPolicy
│   │   └── src/lib.rs
│   ├── clank-golem/        # stub only
│   │   └── src/lib.rs
│   └── clank-grease/       # stub only
│       └── src/lib.rs
└── .cargo/
    └── config.toml         # currently empty; WASM config deferred to Phase 0
```

---

## Crate Responsibilities (as built)

### `clank` (binary)

Entry point only. Constructs `NativeHttpClient` and `RealFs`, injects them, creates a
`ClankShell`, and calls `run_interactive()`. Contains system tests in `tests/shell_basics.rs`.

No business logic. No WASM entry point (deferred to Phase 0).

### `clank-shell`

The active implementation crate for Phase -1. Contains:

- **`process.rs`** — `Process` trait, `ProcessContext`, `ProcessIo` (with real `OpenFile`
  handles sourced from Brush's `ExecutionContext`), `ProcessResult`, `StubProcess`.
- **`builtins.rs`** — Global `RwLock<Option<DispatchTable>>` keyed by command name. Populated
  at startup via `clank_builtins(stub)`. Supports runtime registration and deregistration via
  `register_command` / `deregister_command` (exported from `lib.rs` for Phase 3 use).
  `dispatch_builtin` is the bare fn pointer registered with Brush; it looks up the command,
  builds a `ProcessContext` with I/O from the `ExecutionContext`, and drives the `Process`.
- **`shell.rs`** — `ClankShell` wrapping `brush_core::Shell`. Built via `Shell::builder()` with
  `brush-builtins` defaults (`BuiltinSet::BashMode`) plus clank overrides. `run_interactive()`
  accumulates multi-line input using `parse_string()` to detect incomplete commands before
  executing — handles `if/fi`, `while/done`, multi-line strings, and heredoc buffering.
  `run_line()` drives `run_string()` and maps `ExecutionResult` to an `i32` exit code.
- **`extensions.rs`** — `ClankExtensions` placeholder. `brush-core` 0.4.0 is not generic over
  extensions; this module is reserved for when/if an extension hook is added upstream.

**Deviation from proposed design:** `clank-shell` does not yet contain the transcript, process
table, authorization model, or terminal abstraction. These are Phase 1 and 2 concerns. The
proposed design listed them here for completeness; they are not yet implemented.

### `clank-http`

`HttpClient` trait, `HttpError` enum, `NativeHttpClient` (reqwest + rustls), `MockHttpClient`.

**Addition not in proposed design:** `MockHttpClient` and `MockResponse` are public types,
present in the crate unconditionally (not behind `#[cfg(test)]`), so consumer crates can use
them as `dev-dependencies`. This is the established mock pattern for all future crates.

**Deviation from proposed design:** No `WasiHttpClient`. WASM target deferred to Phase 0.
No logging wrapper (deferred — `/var/log/http.log` logging is a Phase 1+ concern).

### `clank-vfs`

`Vfs` trait, `VfsError`, `DirEntry`, `FileStat`, `RealFs`, `MockVfs`.

**Addition not in proposed design:** `MockVfs` is a public in-memory fake, available as a
`dev-dependency`. Builder-style API: `MockVfs::new().with_file(path, content)`.

**Deviation from proposed design:** No `LayeredVfs`, no `ProcHandler`, no `BinHandler`, no
`McpResourceHandler`. These are Phase 2 and 3 concerns.

### `clank-manifest`

`CommandManifest`, `ExecutionScope`, `AuthorizationPolicy`. Skeleton only — no registry,
no parsing, no manifest derivation. These are Phase 1+ concerns.

### `clank-ask`, `clank-golem`, `clank-grease`

Stub crates. Each contains only a comment indicating the phase in which implementation begins.
Correct `Cargo.toml` dependencies are in place.

---

## Dependency Graph (as built)

```
clank (binary)
  ├── clank-shell
  │     ├── clank-vfs
  │     ├── clank-manifest
  │     ├── clank-http
  │     └── brush-core, brush-parser, brush-builtins, tokio, async-trait, futures, anyhow,
  │         thiserror, tracing
  ├── clank-http
  ├── clank-vfs
  ├── clank-ask       (stub; depends on clank-http, clank-manifest)
  ├── clank-manifest
  ├── clank-golem     (stub; depends on clank-http)
  └── clank-grease    (stub; depends on clank-http, clank-manifest, clank-vfs)

clank-vfs
  (no internal crate dependencies — clank-http dependency deferred to Phase 3)

clank-manifest
  (no internal crate dependencies)
```

**Deviation from proposed design:** The proposed design showed `clank-vfs` depending on
`clank-http` (for `McpResourceHandler` dynamic resource fetching). This dependency is not
present yet — it will be added in Phase 3 when `McpResourceHandler` is implemented.

**Addition not in proposed design:** `clank-shell` directly depends on `clank-http`. This was
needed to wire `NativeHttpClient` through to dispatch (the binary injects it, but `clank-shell`
references the trait). This does not violate the no-cycles constraint and is architecturally
sound.

---

## Brush Integration (key findings)

The proposed design did not specify the Brush integration mechanism in detail. The realized
approach:

- `brush-core` 0.4.0's `Shell` is not generic. `ShellExtensions` trait exists but carries only
  `ErrorFormatter` — no `ProcessSpawner` hook.
- `CommandExecuteFunc` is a bare fn pointer (not a closure). Capturing `Arc<dyn Process>` per
  command is impossible without a side channel.
- **Chosen approach:** Global `RwLock<HashMap<String, Arc<dyn Process>>>`. The bare fn pointer
  `dispatch_builtin` looks up the command by name at call time. Supports runtime
  registration/deregistration (needed for Phase 3 `grease install`).
- All clank commands are registered as Brush builtins by name. Brush checks builtins before
  `$PATH` resolution, so registered names are always handled in-process.
- Multi-line input buffering: `parse_string()` returns `ParseError::ParsingAtEndOfInput` or
  `ParseError::Tokenizing { UnterminatedHereDocuments }` for incomplete input. The interactive
  loop accumulates lines until neither condition holds.

---

## Toolchain and Configuration

`rust-toolchain.toml`: pins `channel = "stable"`.

`.cargo/config.toml`: present but empty. WASM-specific configuration is deferred to Phase 0.

`[workspace.dependencies]`: all shared dependencies declared once in the workspace root
`Cargo.toml`. Individual crate `Cargo.toml` files use `{ workspace = true }`.

`tokio-test` added to workspace dependencies for async doctest support.

---

## Testing Infrastructure (addition not in proposed design)

Not covered in the proposed design. Established in Phase -1:

- **System tests** in `crates/clank/tests/shell_basics.rs` using `assert_cmd`.
- **`MockHttpClient`** in `clank-http`: records requests, returns queued `MockResponse` values.
  Doctest verifies correct behaviour.
- **`MockVfs`** in `clank-vfs`: in-memory file map. Doctest verifies correct behaviour.
- **Testing conventions** documented in `AGENTS.md` covering three levels (unit, crate
  integration, system) with mandatory coverage requirements per behaviour type.

---

## Phase Mapping (updated)

| Phase | Crates with substantive changes |
|---|---|
| -1 *(done)* | `clank` (binary + system tests), `clank-shell` (Brush, Process trait, dispatch, interactive loop), `clank-http` (trait + native impl + mock), `clank-vfs` (trait + RealFs + mock), `clank-manifest` (skeleton) |
| 0 *(deferred)* | `clank-http` (WasiHttpClient), `clank` (WASM entry point) |
| 1 | `clank-shell` (transcript, context builtin, terminal), `clank-ask` (ask, model), `clank-manifest` (initial registry) |
| 2 | `clank-shell` (process table, job control, auth, prompt-user), `clank-vfs` (LayeredVfs, ProcHandler) |
| 3 | `clank-grease`, `clank-manifest` (full), `clank-vfs` (McpResourceHandler) |
| 4 | `clank-golem`, `clank-ask` (ask repl) |
| 5 | All crates (polish, signed registry, compaction, TUI, man pages) |
