---
title: "Project skeleton — minimal buildable Rust workspace"
date: 2026-03-06
author: agent
issue: dev-docs/issues/open/project-skeleton.md
research: []
designs: []
---

## Summary

Establish a minimal Rust workspace with three crates that compiles natively, wires `brush-core`
into a working stdin REPL, and provides the `HttpClient` abstraction seam described in the README.
This unblocks all subsequent implementation work.

## Design Decisions

### Crate decomposition

Three crates, derived directly from the README architecture:

| Crate | Kind | Purpose |
|---|---|---|
| `clank-shell` | binary | Entry point; reads stdin, drives the REPL loop |
| `clank-core` | library | Shell interpreter wiring; owns the `brush-core` integration |
| `clank-http` | library | `HttpClient` trait and `NativeHttpClient` stub (the HTTP abstraction seam) |

`clank-core` and `clank-http` are separate from the start because the README explicitly identifies
HTTP as an abstraction seam with divergent implementations on native vs. WASM. Keeping it isolated
avoids tangling it with shell interpreter logic from the first commit.

**Decision recorded:** No `clank-golem`, `clank-mcp`, or other crates are created in this task.
They are future crates for future tasks. The skeleton deliberately minimises scope.

### brush-core integration depth

The skeleton wires `brush-core` into a working REPL rather than leaving it as a bare dependency.
Rationale: a REPL proves the integration end-to-end (parse, execute, output) and gives the project
a runnable artifact from the first task. A bare dependency would compile but give no confidence
that the wiring is correct.

`brush-interactive` is NOT adopted. The README explicitly replaces it with clank's own
transcript-aware interactive layer. The skeleton implements a minimal stdin read loop directly,
without readline or reedline.

### WASM target

`brush-core` 0.4.0 depends on the `nix` crate, which does not compile for `wasm32-wasip2`. Making
`brush-core` compile for WASM requires replacing the entire process execution layer — a substantial
design task of its own. The WASM target is therefore deferred: the skeleton targets native only.
A separate issue will be filed once the process model design is ready.

The `clank-http` `WasiHttpClient` stub is still included as a type but gated behind
`#[cfg(target_arch = "wasm32")]` so the seam is visible even though it is not yet exercised.

### HTTP client stub

`NativeHttpClient` is a struct that implements `HttpClient` using `reqwest`. In the skeleton it is
a stub: the trait is defined, the struct exists, but no real HTTP calls are made. This establishes
the abstraction boundary the README calls out without requiring a working HTTP layer to land the
skeleton.

## Developer Feedback

No significant design decisions required external input beyond what is captured above. The crate
decomposition, brush-core integration depth, and WASM deferral were discussed with the developer
prior to writing this plan.

## Acceptance Tests

1. `cargo build` exits 0 on native target.
2. `cargo test` exits 0 (even with zero tests initially).
3. Running the binary, typing `echo hello`, and pressing Enter prints `hello` to stdout.
4. Running the binary, typing `exit`, exits with code 0.

## Tasks

- [ ] Create `dev-docs/issues/open/project-skeleton.md`
- [ ] Create `dev-docs/plans/proposed/project-skeleton.md` (this file)
- [ ] Create top-level `Cargo.toml` workspace defining all three member crates
- [ ] Create `clank-http` library crate with `HttpClient` trait and `NativeHttpClient` stub
- [ ] Create `clank-core` library crate with `brush-core` wiring and minimal REPL function
- [ ] Create `clank-shell` binary crate with `main.rs` stdin loop
- [ ] Verify `cargo build` passes on native
- [ ] Verify `cargo test` passes
- [ ] Verify acceptance test 3: `echo hello` prints `hello`
- [ ] Verify acceptance test 4: `exit` exits 0
- [ ] Fill in the **Build & Test** section of `AGENTS.md`
