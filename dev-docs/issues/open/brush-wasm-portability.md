---
title: "brush-core cannot compile to wasm32-wasip2"
date: 2026-03-06
author: agent
---

# brush-core cannot compile to wasm32-wasip2

## Problem

`brush-core 0.4.0` cannot be compiled for the `wasm32-wasip2` target due to hard dependencies on Unix-only system crates. The clank.sh design requires the shell to run as a single WebAssembly component on Golem, but this is currently impossible while `brush-core` is used as a library dependency.

## Impact

The WASM build target — the primary deployment target for clank.sh on Golem — is completely blocked. Every feature that depends on the shell interpreter (`ask`, `grease`, `prompt-user`, and all builtins) cannot be deployed to Golem until this is resolved.

## Context

The following dependencies in `brush-core 0.4.0` / `brush-builtins 0.1.0` prevent compilation on `wasm32-wasip2`:

1. **`nix ^0.30.1`** — Unix POSIX APIs (signals, process groups, `fork`/`exec`, `ioctl`). Does not support `wasm32` targets.
2. **`tokio` process and signal features** — `tokio::process::Command`, `tokio::signal`, SIGTSTP/SIGCHLD/SIGINT handlers. All Unix-only; Tokio has no WASI Preview 2 support (tracked upstream in tokio#6178, unresolved as of early 2026).
3. **`procfs` in `brush-builtins`** — requires the Linux `/proc` filesystem.
4. **`uzers` / `whoami` / `hostname`** — user and host info crates requiring native OS APIs.

Additionally, there is **no pluggable process spawner interface** in `brush-core`. The clank.sh design calls for replacing the process execution layer entirely with an internal async abstraction, but this cannot be done without modifying `brush-core` source — either via a contribution upstream or via a vendored fork.

The `sys/` platform abstraction refactoring (merged in brush PR #735, November 2025) is a necessary precondition but targets Windows, not WASM, and is far from sufficient.

No upstream WASM/WASI work is planned in the `brush` project as of March 2026.

## Resolution Path

One of the following approaches must be evaluated and chosen in a plan:

1. **Upstream contribution** — Design and contribute a pluggable process/signal abstraction interface to `brush-core` that can be swapped out for a WASM-compatible implementation. This is the cleanest long-term path but depends on upstream maintainer cooperation and timeline.

2. **Vendored fork** — Vendor `brush-core` as a workspace crate (under `vendor/brush-core/`) and apply the necessary patches locally. Faster to unblock WASM but creates ongoing maintenance burden for tracking upstream.

3. **Conditional compilation via feature flags** — If upstream is willing, gating all Unix-specific code behind a `native` feature and providing no-op or `wstd`-backed implementations for WASM. This is the least invasive approach but requires upstream buy-in.

## Out of Scope

This issue does not cover the Golem WIT interface, `cargo-component` setup, `golem-rust` integration, or any specific builtin implementation. Those are separate issues to be filed once the WASM compilation blocker is resolved.
