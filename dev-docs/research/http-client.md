---
title: HTTP Client Abstraction — Ecosystem Survey and Seam Design
date: 2026-03-07
author: agent
---

## Purpose

Determine the correct HTTP client libraries for native and `wasm32-wasip2`,
how to select between them without changing the project structure, and what
the trait abstraction should look like for use by `ask`.

---

## The Core Constraint

The same Rust source must compile to both native and `wasm32-wasip2` without
any structural change — no separate crates, no different `Cargo.toml` files,
no `wac` composition tooling, no `cargo component`. Just `cargo build` on both
targets.

This matches the README's stated design:

> "Conditional compilation handles these seams, backed by a small trait with
> two implementations where needed."

---

## Ecosystem Survey

### `reqwest`

- The standard Rust HTTP client. Async + blocking, TLS via rustls/native-tls.
- **Native: ✅ Works perfectly.**
- **`wasm32-wasip2`: ❌** PR #2453 adding `wasip2` support is still a draft
  (March 2026). TLS implementations (ring, AWS-LC) do not link on WASM. The
  PR author noted in December 2024 that `tokio/time` also breaks on WASM.
  Not merged, no timeline.

### `golem-wasi-http` (by Golem maintainer vigoo, v0.2.0, March 2026)

- A fork of reqwest that dropped all non-WASI backends entirely.
- Backed by `wasip2 = "1.0.2"` (raw WASI bindings) as a **non-optional dep**.
- **`wasm32-wasip2`: ✅ Correct implementation for Golem and any WASI runtime.**
- **Native: ❌** The `wasip2` crate's `extern` declarations reference host
  functions that do not exist on native. Link error at native compile time.
- API is reqwest-like: `Client::new().post(url).json(&body).send()?`
- Has `json` feature for serde integration.
- Actively maintained — last updated March 3, 2026. Endorsed by Golem docs.

### `golem-ai` LLM components (WASM composition approach)

- Pre-built `.wasm` components for Anthropic, OpenAI etc. composed via `wac`.
- Requires `cargo component`, WIT imports, and a separate build step.
- **Ruled out.** Requires a fundamentally different project structure. The
  README explicitly says "zero application-level changes" between native and
  Golem. A composition-based approach violates this.

### `waki`

- WASI HTTP client built on `wit-bindgen`. Same native incompatibility as
  `golem-wasi-http`. Less reqwest-compatible, less actively maintained.
  Not selected.

---

## The Solution: Cargo Target-Specific Dependencies

Cargo supports platform-specific dependencies via:

```toml
[target.'cfg(not(target_arch = "wasm32"))'.dependencies]
reqwest = { version = "0.12", features = ["json"] }

[target.'cfg(target_arch = "wasm32")'.dependencies]
golem-wasi-http = { version = "0.2", features = ["json"] }
```

Both crates are in the same `Cargo.toml`. Cargo selects the right one at
compile time based on the target architecture. No structural change, no new
workspace members, no extra tooling.

This is idiomatic Rust for exactly this kind of platform seam. It is how
`tokio` itself gates certain features on WASM, how `rand` gates its OS entropy
source, etc.

---

## The Trait Design

`clank-http` exposes a single trait and a `DefaultHttpClient` type alias:

```rust
/// A named type for an HTTP header (name, value pair).
pub struct HttpHeader {
    pub name: String,
    pub value: String,
}

/// The response from an HTTP request.
pub struct HttpResponse {
    /// HTTP status code.
    pub status: u16,
    /// Response body as a UTF-8 string.
    pub body: String,
}

/// An error from an HTTP request.
#[derive(Debug, thiserror::Error)]
pub enum HttpError {
    #[error("request failed: {0}")]
    Request(String),
    #[error("response body could not be decoded: {0}")]
    Decode(String),
}

/// An HTTP client capable of making outgoing requests.
/// Implemented by NativeHttpClient (native) and WasiHttpClient (wasm32-wasip2).
/// Call sites hold Arc<dyn HttpClient> — no #[cfg] needed at call sites.
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

Key decisions:

**Async, not sync.** `ask` lives in an async tokio context. An async trait
composes naturally without `spawn_blocking`. `reqwest`'s async client is
the primary implementation on native. `golem-wasi-http` also supports async
via `wstd` (optional feature). Async is the correct default for I/O-bound
operations.

**`Arc<dyn HttpClient>` at call sites.** Call sites hold `Arc<dyn HttpClient>`
with no `#[cfg(target_arch)]` needed. The concrete type is constructed once
at startup and passed down. This is cleaner than a `DefaultHttpClient` type
alias that leaks the concrete type everywhere.

**`post_json` only for MVP.** `ask` needs exactly one operation: POST with
a JSON body, receive a JSON response. No multipart, no streaming, no GET.
Additional methods can be added when needed.

**Named types, not tuples.** `HttpHeader`, `HttpResponse`, `HttpError` are
named structs — per AGENTS.md code conventions. No `(String, String)` for
headers.

---

## Implementation

### Native (`src/native.rs`)

```rust
#[cfg(not(target_arch = "wasm32"))]
pub struct NativeHttpClient {
    client: reqwest::Client,
}

#[async_trait]
impl HttpClient for NativeHttpClient {
    async fn post_json(&self, url, headers, body) -> Result<HttpResponse, HttpError> {
        let mut req = self.client.post(url).body(body.to_string());
        for h in headers {
            req = req.header(&h.name, &h.value);
        }
        let resp = req.send().map_err(|e| HttpError::Request(e.to_string()))?;
        let status = resp.status().as_u16();
        let body = resp.text().map_err(|e| HttpError::Decode(e.to_string()))?;
        Ok(HttpResponse { status, body })
    }
}
```

### WASM (`src/wasm.rs`)

A stub returning `HttpError::Unavailable` — marks the seam where a
`golem-wasi-http` implementation will be substituted when the WASM target
is addressed:

```rust
#[cfg(target_arch = "wasm32")]
pub struct WasiHttpClient;

#[async_trait]
impl HttpClient for WasiHttpClient {
    async fn post_json(&self, _url, _headers, _body) -> Result<HttpResponse, HttpError> {
        Err(HttpError::Unavailable)
    }
}
```

### Constructing the client at startup

```rust
// In ClankShell::new() or main():
let http: Arc<dyn HttpClient> = Arc::new(NativeHttpClient::new()?);
```

No `#[cfg]` needed at call sites — `Arc<dyn HttpClient>` works on both targets.

---

## Cargo.toml for `clank-http`

```toml
[package]
name = "clank-http"
version = "0.1.0"
edition = "2021"

[dependencies]
async-trait = "0.1"
thiserror = "2"

[target.'cfg(not(target_arch = "wasm32"))'.dependencies]
reqwest = { version = "0.12", features = ["json"] }

[target.'cfg(target_arch = "wasm32")'.dependencies]
golem-wasi-http = { version = "0.2", features = ["json", "async"] }
```

---

## Testing Strategy

Unit tests on the trait and types (no HTTP): compile on both targets.

Integration tests for `ReqwestClient` on native: use `mockito` or
`httptest` to spin up a local HTTP server, call `post_json`, assert on
response. These tests only compile/run on native (`#[cfg(not(target_arch = "wasm32"))]`).

WASM integration testing is deferred — requires a WASI runtime and a mock
HTTP server accessible from inside Wasmtime. Out of scope for this task.

---

## Conclusions

1. **`reqwest` (blocking) on native, `golem-wasi-http` on WASM** — selected.
2. **Cargo target-specific deps** (`[target.'cfg(...)'.dependencies]`) — no
   structural change, no tooling change, just `cargo build`.
3. **Async `post_json` trait** — composes naturally with tokio, no `spawn_blocking`.
4. **`Arc<dyn HttpClient>` at call sites** — no `#[cfg]` needed downstream.
5. **Named types throughout** — `HttpHeader`, `HttpResponse`, `HttpError`.
6. **`clank-http` crate** — new workspace member, depended on by `ask`.
7. **WASM stub** returns `HttpError::Unavailable` — marks the seam cleanly.
