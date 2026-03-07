---
title: No HTTP client abstraction — ask cannot make model API calls on native or WASM
date: 2026-03-07
author: agent
---

## Summary

`ask` must make outgoing HTTP requests to AI model provider APIs (Anthropic,
OpenAI, etc.). There is currently no HTTP client in the codebase. Furthermore,
the HTTP client used must work on both native Rust and `wasm32-wasip2` — and
these two targets require fundamentally different implementations:

- **Native:** `reqwest` (tokio-backed, TLS via rustls/native-tls)
- **WASM:** `wasi:http` outgoing handler (WASI HTTP 0.2 — provided by the host runtime)

This is the HTTP seam the README describes:

> "Seams appear where crate support diverges — primarily HTTP clients
> (`reqwest` on native, `wstd` or equivalent on wasm-wasi). Conditional
> compilation handles these seams, backed by a small trait with two
> implementations where needed."

---

## State of the Ecosystem (March 2026)

### `reqwest` for `wasm32-wasip2`

`reqwest` PR #2453 ("feat(wasm): add support for the stable target
wasm32-wasip2") is still a **draft** as of early 2026. It has not been merged.
The author noted in December 2024 that it needs integration with a
wasm-compatible async runtime before it can be ready for review. A separate
commenter confirmed that `tokio/time` breaks on WASM if naively depended on,
and TLS implementations (ring, AWS-LC) do not link on `wasm32-wasip2`.

**Conclusion: `reqwest` cannot be used on `wasm32-wasip2` today.**

### `golem-wasi-http` (by the Golem team, v0.2.0, March 2026)

A fork of reqwest that dropped all non-WASI backends and provides a
reqwest-like API backed entirely by `wasi:http`. Published by Golem's own
maintainer (Daniel Vigovszky). Actively maintained — updated March 3, 2026.

- Supports blocking and async (via `wstd`)
- JSON support via `serde_json` feature
- No `nix`, no `libc`, no TLS stack — host runtime provides all of this
- `wasm32-wasip2` only — does not compile on native

**Conclusion: `golem-wasi-http` is the correct WASM implementation.**

### Golem's own recommendation

Golem's HTTP docs page only shows TypeScript for HTTP. The Rust section links
to `golem-wasi-http` as the recommended Rust HTTP client for Golem components.

### `waki`

Another WASI HTTP client (`waki` 0.5.1) built on `wit-bindgen`. Lighter weight
but less reqwest-compatible. A viable alternative but `golem-wasi-http` is more
directly endorsed by the Golem team.

---

## The Seam Design

The README prescribes a **trait-based seam**:

```rust
/// An HTTP client capable of making outgoing requests.
/// Two implementations: ReqwestClient (native) and WasiHttpClient (WASM).
#[async_trait]
pub trait HttpClient: Send + Sync {
    async fn post_json(
        &self,
        url: &str,
        headers: &[HttpHeader],
        body: &str,
    ) -> Result<HttpResponse, HttpError>;
}

pub struct HttpHeader {
    pub name: String,
    pub value: String,
}

pub struct HttpResponse {
    pub status: u16,
    pub body: String,
}
```

All application code (including `ask`) calls `dyn HttpClient` or a generic
`T: HttpClient`. The implementation is selected at compile time:

```rust
#[cfg(not(target_arch = "wasm32"))]
pub use native::ReqwestClient as DefaultHttpClient;

#[cfg(target_arch = "wasm32")]
pub use wasm::WasiHttpClient as DefaultHttpClient;
```

---

## Where This Lives

A new `clank-http` library crate in the workspace:

```
clank.sh/
└── clank-http/
    ├── Cargo.toml
    └── src/
        ├── lib.rs         ← HttpClient trait, HttpHeader, HttpResponse, HttpError
        ├── native.rs      ← ReqwestClient (cfg: not wasm32)
        └── wasm.rs        ← WasiHttpClient using golem-wasi-http (cfg: wasm32)
```

`clank-http` is depended on by `ask` (future crate or module). It is not
depended on by anything else in the current codebase.

---

## Scope for This Issue

1. Define the `HttpClient` trait with the minimum API needed by `ask`:
   `post_json(url, headers, body) -> HttpResponse`
2. Implement `ReqwestClient` for native (`#[cfg(not(target_arch = "wasm32"))]`)
3. Implement `WasiHttpClient` for WASM (`#[cfg(target_arch = "wasm32")]`) using `golem-wasi-http`
4. Export `DefaultHttpClient` as the correct implementation for the current target
5. Write unit tests for the native implementation against a mock/local server

`ask` itself (using `HttpClient` to call model APIs) is a separate issue.

---

## Acceptance Condition

- `cargo build` succeeds on native with `ReqwestClient` as the implementation
- The `HttpClient` trait is defined and `DefaultHttpClient` resolves correctly
- Unit/integration tests verify that `ReqwestClient` can make a real POST
  request (against httpbin or similar) on native
- `clank-http` is WASM-compatible: no `nix`, no `libc` in the shared trait
  code; native-only code strictly gated behind `#[cfg(not(target_arch = "wasm32"))]`
