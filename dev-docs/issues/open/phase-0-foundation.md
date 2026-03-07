---
title: "Phase 0: Add wasm32-wasip2 target and dual-target abstractions"
date: 2026-03-06
author: agent
deferred: true
deferred_date: 2026-03-06
deferred_reason: >
  Decision made to build all functionality on native only until further notice.
  The wasm32-wasip2 target introduces unresolved complexity (brush-core nix
  dependency, tokio vs WASM async executor mismatch) that is not worth
  addressing until the feature set is more complete. This issue remains open
  but is explicitly not on the current roadmap.
---

# Phase 0: Add wasm32-wasip2 target and dual-target abstractions

## Problem

After Phase -1, the shell runs on native but does not compile on `wasm32-wasip2`. The codebase
uses `reqwest` directly (no WASM-safe alternative) and Brush pulls in the `nix` crate (which does
not compile on WASM). The `clank` binary has no WASM entry point.

## Capability Gap

- `cargo build --target wasm32-wasip2` fails.
- `clank-http` has no `WasiHttpClient` and no `#[cfg]` guards.
- Brush's `nix` dependency is not excluded at the process trait boundary.
- No WIT-exported WASM component entry point.
- No CI job for the WASM target.

## Deliverables

The entire workspace compiles on both `native` and `wasm32-wasip2`. Feature parity between the
two targets is identical to Phase -1 — this phase adds no new user-visible features, only
portability.

Concretely:

- `rust-toolchain.toml` updated to a toolchain with `wasm32-wasip2` component model async support
- `WasiHttpClient` added to `clank-http` wrapping `wstd`; both implementations behind `#[cfg]`
  guards; `clank` binary injects the correct one at startup
- `nix` crate excluded at the process trait boundary via `#[cfg]` guard; no `nix` code reachable
  on the WASM target
- `clank` binary has a `lib.rs` with WIT-exported functions as the WASM component entry point,
  alongside the existing `main.rs` for native
- `.cargo/config.toml` configured for both targets
- `cargo build --target wasm32-wasip2` passes
- `cargo clippy --target wasm32-wasip2 -- -D warnings` passes
- CI runs both `cargo build` (native) and `cargo build --target wasm32-wasip2`

## Out of Scope

Transcript, `ask`, process table, virtual filesystem, MCP, Golem, `grease`, tab completion,
authorization. All addressed in later phases.

## Dependency

Requires Phase -1 to be complete.
