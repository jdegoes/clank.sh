---
title: "Project skeleton — realized design"
date: 2026-03-06
author: agent
---

## Overview

This document records the realized design of the project skeleton as implemented. It supersedes
any prior design intent for the workspace structure, crate decomposition, and dependency choices.

## Workspace Structure

```
Cargo.toml                  workspace root; resolver = "2"
clank-shell/
  Cargo.toml
  src/main.rs               binary entry point
clank-core/
  Cargo.toml
  src/lib.rs                public API (re-exports Repl)
  src/repl.rs               brush-core REPL wiring
clank-http/
  Cargo.toml
  src/lib.rs                HttpClient trait + NativeHttpClient + WasiHttpClient stub
```

## Crate Responsibilities

### `clank-shell`

Binary crate. Initialises `tracing-subscriber` (defaults to `WARN` level; overridable via
`RUST_LOG`), constructs a `clank_core::Repl`, and calls `repl.run()`. The binary name is `clank`.

No application logic lives here. It is intentionally thin.

### `clank-core`

Library crate. Currently contains one public module: `repl`.

**`repl::Repl`** — A minimal read-eval-print loop that:

1. Prints a `$ ` prompt to stdout.
2. Reads one line from stdin via `BufRead::read_line`.
3. Adds the line to `brush-core` history (so `!!` and `!n` work).
4. Executes the line via `brush_core::Shell::run_string`.
5. Prints any execution errors to stderr.
6. Loops until EOF.
7. On EOF, calls `shell.on_exit()` to run EXIT traps.

On `wasm32`, `Repl` is a stub that prints a "not yet implemented" message and returns. This keeps
the crate structure consistent across targets without causing a compile failure.

**brush-core configuration:**

- Builder API: `brush_core::Shell::builder()`
- Builtins: `brush_builtins::BuiltinSet::BashMode` via `ShellBuilderExt::default_builtins`
- `no_profile(true)`, `no_rc(true)` — no `.bashrc` or system profile sourced on startup
- `interactive(true)` — shell is in interactive mode
- `shell_name("clank")`, `shell_product_display_str("clank.sh")`

`brush-interactive` is not used. The stdin read loop is implemented directly.

### `clank-http`

Library crate. Defines the HTTP abstraction seam.

**`HttpClient` trait** (from `async-trait`):

```rust
pub trait HttpClient: Send + Sync {
    async fn get(&self, url: &str) -> Result<HttpResponse, HttpError>;
    async fn post_json(&self, url: &str, body: &[u8]) -> Result<HttpResponse, HttpError>;
}
```

**`HttpResponse`**: `{ status: u16, body: Vec<u8> }` with `body_str()` convenience method.

**`HttpError`**: `RequestFailed(String)` and `Unavailable`.

**`NativeHttpClient`** (native only, `cfg(not(target_arch = "wasm32"))`): wraps `reqwest::Client`.
Uses `rustls-tls` feature to avoid an OpenSSL system dependency. No real HTTP calls are made in
the skeleton; the client constructs successfully and compiles.

**`WasiHttpClient`** (wasm32 only): always returns `HttpError::Unavailable`. Marks the seam for
a future `wstd`-backed implementation.

All call sites should use `Arc<dyn HttpClient>` — no `#[cfg(target_arch)]` at call sites.

## Dependency Choices

| Dependency | Version | Reason |
|---|---|---|
| `brush-core` | 0.4.0 | Embeddable bash-compatible shell interpreter |
| `brush-parser` | 0.3.0 | Required transitive dependency of brush-core; declared explicitly |
| `brush-builtins` | 0.1.0 | Registers the full bash builtin set (exit, cd, etc.) |
| `tokio` | 1.48, features = ["full"] | Async runtime; required by brush-core and the REPL |
| `reqwest` | 0.12, rustls-tls | HTTP client; rustls avoids OpenSSL system dependency on NixOS |
| `anyhow` | 1.0 | Error propagation in application code |
| `thiserror` | 2.0 | Typed error definitions in library code |
| `async-trait` | 0.1 | Required for async methods in the HttpClient trait |
| `tracing` | 0.1 | Structured logging |
| `tracing-subscriber` | 0.3 | Subscriber setup in the binary |

## Target Gating

`brush-core`, `brush-parser`, `brush-builtins` are gated behind
`[target.'cfg(not(target_arch = "wasm32"))'.dependencies]` in `clank-core/Cargo.toml`. This
prevents a compile failure on `wasm32-wasip2` where `brush-core`'s `nix` dependency would fail.

`reqwest` and `tokio` (in `clank-http`) are similarly gated for the same reason.

## Deviations from Approved Plan

None. Implementation followed the plan exactly. One naming correction: `BuiltinSet::Default` does
not exist in `brush-builtins` 0.1.0 — the correct variant is `BuiltinSet::BashMode`. The plan
referred to a "default set of builtins" conceptually; `BashMode` is the correct concrete variant.

## Acceptance Test Results

All four acceptance tests pass:

1. `cargo build` — exits 0 ✓
2. `cargo test` — 3 tests, 0 failures ✓
3. `echo hello` — prints `hello` ✓
4. `exit` — exits with code 0 ✓

## Open Decisions / Future Work

- **WASM process model**: the biggest deferred item. Once designed, `brush-core` is replaced at
  the `Repl` boundary with a WASM-compatible implementation. The stub in `repl.rs` marks the seam.
- **WasiHttpClient**: currently always returns `Unavailable`. Will be replaced with a `wstd`-backed
  implementation once the WASM target is addressed.
- **Transcript-aware interactive layer**: the `Repl` is a minimal placeholder. The full design
  (sliding-window transcript, `context` builtin, prompt rendering) is a future task.
- **Code Conventions**: the `AGENTS.md` "Code Conventions" section remains `_To be filled in_` and
  should be addressed in a dedicated issue once patterns emerge from the first few implementation
  tasks.
