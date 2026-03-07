---
title: Implement HTTP Client Abstraction (clank-http)
date: 2026-03-07
author: agent
issue: dev-docs/issues/open/http-client.md
research:
  - dev-docs/research/http-client.md
designs: []
---

## Summary

Introduce a `clank-http` library crate providing an `HttpClient` trait with
two implementations selected at compile time via Cargo target-specific
dependencies — `reqwest` (blocking) on native and `golem-wasi-http` on
`wasm32-wasip2`. No structural change to the project. Just `cargo build` on
both targets.

## Developer Feedback

- No structural change to the project — no `cargo component`, no WIT imports,
  no `wac` composition. Same workspace, same `cargo build` command.
- `golem-ai` LLM composition approach ruled out — requires different project
  structure.
- Async `post_json` trait with `async_trait` — composes naturally with tokio.
- Call sites hold `Arc<dyn HttpClient>` — no `#[cfg]` needed downstream.
- Named types throughout: `HttpHeader`, `HttpResponse`, `HttpError`.

## Crate Structure

```
clank.sh/
├── Cargo.toml              ← add clank-http to members
└── clank-http/
    ├── Cargo.toml          ← target-specific deps: reqwest / golem-wasi-http
    └── src/
        ├── lib.rs          ← HttpClient trait, HttpHeader, HttpResponse, HttpError
        ├── native.rs       ← ReqwestClient (#[cfg(not(target_arch = "wasm32"))])
        └── wasm.rs         ← WasiHttpClient (#[cfg(target_arch = "wasm32")])
```

## Types

### `HttpHeader`
```rust
pub struct HttpHeader {
    pub name: String,
    pub value: String,
}
```

### `HttpResponse`
```rust
pub struct HttpResponse {
    pub status: u16,
    pub body: String,
}
```

### `HttpError`
```rust
#[derive(Debug, thiserror::Error)]
pub enum HttpError {
    #[error("request failed: {0}")]
    Request(String),
    #[error("response body could not be decoded: {0}")]
    Decode(String),
}
```

### `HttpClient` trait
```rust
#[async_trait]
pub trait HttpClient: Send + Sync {
    async fn post_json(
        &self,
        url: &str,
        headers: &[HttpHeader],
        body: &str,
    ) -> Result<HttpResponse, HttpError>;
}
```

Call sites hold `Arc<dyn HttpClient>` — no `#[cfg(target_arch)]` needed downstream.

## Cargo.toml for `clank-http`

```toml
[dependencies]
async-trait = "0.1"
thiserror = "2"

[target.'cfg(not(target_arch = "wasm32"))'.dependencies]
reqwest = { version = "0.12", features = ["json"] }

[target.'cfg(target_arch = "wasm32")'.dependencies]
golem-wasi-http = { version = "0.2", features = ["json", "async"] }

[dev-dependencies]
mockito = "1"          # local mock HTTP server for native integration tests
```

## `NativeHttpClient` (native)

Uses `reqwest::Client` (async). Constructed once and held behind `Arc<dyn HttpClient>`.
`post_json` sets `Content-Type: application/json`, adds caller-supplied headers,
sends the body, reads the response body as text. Fully async — composes directly
with tokio.

## `WasiHttpClient` (WASM stub)

Returns `Err(HttpError::Unavailable)`. Marks the seam cleanly — the WASM
implementation will be substituted when the WASM target is addressed.

## Acceptance Tests

1. `cargo build` succeeds on native — `NativeHttpClient` compiles, `WasiHttpClient`
   is not linked in the native binary.
2. Unit tests on `HttpHeader`, `HttpResponse`, `HttpError` pass on native.
3. Integration test: `NativeHttpClient::post_json` makes a real POST to a
   `mockito` local server and returns the correct status and body.
4. `cargo clippy --all-targets -- -D warnings` passes.
5. `Arc<dyn HttpClient>` is publicly exported and usable by downstream crates.

## Tasks

- [ ] Add `clank-http` to workspace `Cargo.toml` members
- [ ] Create `clank-http/Cargo.toml` with `async-trait`, `thiserror`, and target-specific deps
- [ ] Define `HttpHeader`, `HttpResponse`, `HttpError` in `clank-http/src/lib.rs`
- [ ] Define async `HttpClient` trait in `clank-http/src/lib.rs`
- [ ] Implement `NativeHttpClient` in `clank-http/src/native.rs` using `reqwest` async client
- [ ] Implement `WasiHttpClient` stub in `clank-http/src/wasm.rs` returning `HttpError::Unavailable`
- [ ] Write unit tests for `HttpHeader`, `HttpResponse`, `HttpError`
- [ ] Write integration test: `NativeHttpClient::post_json` POST against `mockito` server
- [ ] Verify `cargo test` and `cargo clippy` pass
