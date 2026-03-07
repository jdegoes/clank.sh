---
title: "Phase -1: Native-only foundation — Cargo workspace, Brush integration, process trait"
date: 2026-03-06
author: agent
---

# Phase -1: Native-only foundation

## Problem

No implementation exists. Before WASM targets, dual-target abstractions, or any user-visible
feature can be considered, the project needs a working Cargo workspace that compiles and runs on
native, a shell that starts and executes basic commands, and the core architectural abstractions
that all later phases depend on.

Targeting native only at this stage removes the WASM compilation constraints from the critical
path. This tightens the feedback loop and lets the core shell architecture be validated without
simultaneously fighting `wasm32-wasip2` toolchain and dependency issues.

## Capability Gap

- No `Cargo.toml`, no crates, no source files.
- No way to compile or run the shell.
- No Brush integration.
- No internal `Process` trait.
- No terminal layer.

## Deliverables

A native shell binary that starts, accepts commands, executes Brush builtins, and runs simple
shell scripts. The internal process abstraction is defined. The crate structure matches the
proposed workspace design so that later phases can add to it without restructuring.

Concretely:

- Cargo workspace with the crate skeleton from the workspace design
  (`clank`, `clank-shell`, `clank-http`, `clank-vfs`, `clank-ask`, `clank-manifest`,
  `clank-golem`, `clank-grease`) — most crates are stubs at this stage
- `rust-toolchain.toml` pinning a stable toolchain; WASM target support deferred
- `brush-parser` and `brush-core` integrated into `clank-shell`; `brush-builtins` selectively
  adopted; `brush-interactive` replaced with a minimal stdin/stdout interactive loop
- Internal `Process` trait defined in `clank-shell` with stub implementations for each process
  type (builtins, scripts, prompts, Golem agent invocations) — stubs return a clear "not yet
  implemented" error
- `clank-http` contains only `NativeHttpClient` wrapping `reqwest`; no `WasiHttpClient`,
  no `#[cfg]` guards yet (added when WASM is introduced)
- `clank` binary wires up `NativeHttpClient`, starts `clank-shell`, drives the interactive loop
- Shell starts and basic commands work: `echo`, `ls`, `cd`, `export`, `pwd`, pipes, redirections,
  `&&`, `||`, `;`, here-documents
- `cargo build`, `cargo test`, `cargo clippy --all-targets -- -D warnings`, and
  `cargo fmt --check` all pass

## Explicitly Out of Scope

- `wasm32-wasip2` target — deferred to Phase 0
- `WasiHttpClient` and `#[cfg]` target guards — deferred to Phase 0
- `nix` crate exclusion for WASM — deferred to Phase 0
- CI for WASM — deferred to Phase 0
- Transcript, `ask`, process table, virtual filesystem, MCP, Golem, `grease`, tab completion,
  authorization — all addressed in later phases
