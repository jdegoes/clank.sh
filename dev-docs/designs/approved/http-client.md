---
title: HTTP Client Abstraction (clank-http) — Realized Design
date: 2026-03-07
author: agent
---

## Overview

This document records the `clank-http` crate as actually built. It supersedes
any prior approved design for this area (none existed).

---

## What Was Built

A new `clank-http` library crate providing an `HttpClient` trait with
platform-specific implementations selected at compile time via Cargo
target-specific dependencies. No structural change to the project — same
workspace, same `cargo build` command on both targets.

---

## Workspace Structure

```
clank.sh/
├── Cargo.toml              ← clank-http added to members
└── clank-http/
    ├── Cargo.toml          ← async-trait, thiserror; reqwest (native) / golem-wasi-http (wasm)
    └── src/
        ├── lib.rs          ← HttpClient trait, RequestHeader, HttpResponse, HttpError
        ├── native.rs       ← NativeHttpClient (cfg: not wasm32)
        └── wasm.rs         ← WasiHttpClient stub (cfg: wasm32)
```

---

## Types

### `RequestHeader`

```rust
pub struct RequestHeader {
    pub name: String,   // e.g. "Authorization"
    pub value: String,  // e.g. "Bearer sk-..."
}
```

Named type — per AGENTS.md conventions, no `(String, String)` tuples.
`RequestHeader::new(name, value)` convenience constructor.

### `HttpResponse`

```rust
pub struct HttpResponse {
    pub status: u16,
    pub body: String,
}
```

### `HttpError`

```rust
pub enum HttpError {
    Request(String),    // network or transport failure
    Decode(String),     // response body could not be decoded as UTF-8
    Unavailable,        // HTTP client not available on this target (WASM stub)
}
```

### `HttpClient` Trait

```rust
#[async_trait]
pub trait HttpClient: Send + Sync {
    async fn post_json(
        &self,
        url: &str,
        headers: &[RequestHeader],
        body: &str,
    ) -> Result<HttpResponse, HttpError>;
}
```

Call sites hold `Arc<dyn HttpClient>` — no `#[cfg(target_arch)]` needed downstream.

---

## Platform-Specific Dependencies

```toml
[target.'cfg(not(target_arch = "wasm32"))'.dependencies]
reqwest = { version = "0.12", features = ["json"] }
tokio   = { version = "1",    features = ["full"] }

[target.'cfg(target_arch = "wasm32")'.dependencies]
golem-wasi-http = { version = "0.2", features = ["json"] }
```

Cargo selects the correct crate at compile time. No structural change, no extra
tooling — just `cargo build`.

---

## `NativeHttpClient`

`reqwest::Client` (async) constructed once, held behind `Arc<dyn HttpClient>`.
`post_json` sets `Content-Type: application/json`, applies caller-supplied
headers, sends the body, reads the response body as text.

```rust
pub struct NativeHttpClient {
    inner: reqwest::Client,
}
```

`NativeHttpClient::new()` returns `Result<Self, HttpError>` to surface
construction failures. `Default` impl panics on failure — acceptable for
production use where HTTP is expected to be available.

---

## `WasiHttpClient`

A stub that always returns `Err(HttpError::Unavailable)`. It marks the WASM
HTTP seam described in the README — the real implementation will use
`golem-wasi-http` once the WASM target is addressed.

---

## Test Coverage

| Test | What it verifies |
|---|---|
| `request_header_new` | `RequestHeader::new` sets name and value correctly |
| `http_response_fields` | `HttpResponse` fields are accessible |
| `http_error_display` | `HttpError` variants produce readable messages |
| `native_client_constructs` | `NativeHttpClient::new()` succeeds |
| `native_client_is_arc_compatible` | `Arc<dyn HttpClient>` compiles and holds the client |
| `post_json_against_mock_server` | Real POST to a `mockito` server returns 200 + body |
| `post_json_sends_custom_headers` | Custom headers are forwarded to the server |

**7 tests, all passing. Clippy clean.**

---

## Deviations from the Approved Plan

- Plan listed `HttpHeader` as the header type name. Implementation uses
  `RequestHeader` — more specific, avoids ambiguity with response headers.
- The `DefaultHttpClient` type alias was dropped in favour of `Arc<dyn HttpClient>`
  at call sites — cleaner and requires no `#[cfg]` downstream.
