---
title: "Initial Rust Workspace Bootstrap"
date: 2026-03-06
author: agent
issue: "dev-docs/issues/open/initial-workspace-bootstrap.md"
research:
  - "dev-docs/research/brush-embedding-api.md"
  - "dev-docs/research/wasm-toolchain-and-golem.md"
---

# Initial Rust Workspace Bootstrap

## Originating Issue

No buildable Rust workspace exists — see `dev-docs/issues/open/initial-workspace-bootstrap.md`.

## Research Consulted

- `dev-docs/research/brush-embedding-api.md` — Brush embedding API, crate versions, and WASM/WASI blocker analysis.
- `dev-docs/research/wasm-toolchain-and-golem.md` — wasm32-wasip2 toolchain setup, `wstd`, and Golem component model.

## Designs Referenced

None exist yet. This plan produces the first realized design (workspace structure) upon completion.

## Developer Feedback

Consulted on two open questions from the research:

- **Brush WASM blocker:** `brush-core` cannot compile to `wasm32-wasip2` today due to hard dependencies on `nix`, Tokio process/signal APIs, and Linux-specific crates. No upstream WASM work is planned. Decision: **target native only for this issue; file a separate issue for WASM portability after the workspace exists.** The `rust-toolchain.toml` will declare `wasm32-wasip2` as a known future target but no WASM build is attempted here.

- **Golem scaffolding scope:** Decision: **plain Cargo workspace only.** No `cargo-component`, no WIT world, no `golem-rust` dependency. Golem integration is a separate issue.

## Approach

Create a minimal but real Cargo workspace at the repository root that:

1. Compiles successfully on any native target (x86_64 or aarch64, Linux or macOS) 
2. Contains a single `clank-shell` crate that embeds `brush-core` and boots a shell that can run a trivial command (`echo hello`)
3. Has correct toolchain pinning (`rust-toolchain.toml`) and cargo configuration (`.cargo/config.toml`)
4. Stubs the `clank-http` crate with the `HttpClient` trait (native impl only, no WASM impl yet) so the target-conditional compilation pattern is established
5. Includes `cargo test` passing, `cargo clippy -- -D warnings` passing, and `cargo fmt --check` passing

The WASM build target and Golem integration are explicitly deferred. A new issue will be filed at the end of this plan to track the Brush WASM portability work.

## Workspace Structure

```
Cargo.toml                  # [workspace] members
Cargo.lock
rust-toolchain.toml
.cargo/
  config.toml
clank-shell/
  Cargo.toml
  src/
    main.rs
clank-http/
  Cargo.toml
  src/
    lib.rs                  # HttpClient trait + NativeHttpClient stub
```

### `clank-shell`

- Depends on `brush-core 0.4`, `brush-parser 0.3`, `brush-builtins 0.1`, `tokio 1` (full features)
- `main.rs` boots a shell with default extensions and executes `echo hello` to validate the integration
- Binary target: `clank`

### `clank-http`

- Standalone library crate, no shell dependency
- Defines the `HttpClient` trait: `async fn get(&self, url: &str) -> Result<HttpResponse, HttpError>`
- Provides `NativeHttpClient` using `reqwest` (native target only, feature-gated)
- WASM impl is a compile-error stub with a clear `todo!()` and comment pointing to the future issue
- This establishes the pattern described in the design: no `#[cfg(target_arch)]` at call sites

## Acceptance Tests

- `cargo build` succeeds in the workspace root (native target)
- `cargo run -p clank-shell` exits 0 and prints `hello`
- `cargo test --workspace` passes
- `cargo clippy --workspace -- -D warnings` passes
- `cargo fmt --check --workspace` passes
- `clank-http` compiles and its unit test confirms `NativeHttpClient::get` returns an `HttpResponse` for a reachable URL

## Tasks

- [ ] Create `rust-toolchain.toml` pinning stable Rust with `wasm32-wasip2` declared as a known target
- [ ] Create `.cargo/config.toml` with `wasm32-wasip2` per-target flags (`opt-level=z`, `panic=abort`); no global build target override
- [ ] Create root `Cargo.toml` as a workspace with `members = ["clank-shell", "clank-http"]`
- [ ] Create `clank-http` crate with `HttpClient` trait, `HttpResponse`, `HttpError`, and native `reqwest`-backed `NativeHttpClient`
- [ ] Create `clank-shell` crate that embeds `brush-core`, registers no custom builtins yet, and runs `echo hello` in a test
- [ ] Write `clank-shell/src/main.rs` that boots a shell and runs a command passed via argv (or `echo hello` as default)
- [ ] Verify `cargo build --workspace` succeeds on native
- [ ] Verify `cargo test --workspace` passes
- [ ] Verify `cargo clippy --workspace -- -D warnings` passes
- [ ] Verify `cargo fmt --check --workspace` passes
- [ ] File a new issue for Brush WASM portability (the process abstraction layer work)
