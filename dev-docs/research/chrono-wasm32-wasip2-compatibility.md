---
title: "chrono crate compatibility with wasm32-wasip2"
date: 2026-03-07
author: agent
---

# chrono crate compatibility with wasm32-wasip2

## Question

Can `chrono` (specifically `Utc::now()` and `DateTime<Utc>`) be used in a
crate that must eventually compile to `wasm32-wasip2` without requiring
wasm-bindgen, js-sys, or any browser-specific workarounds?

## Conclusion

**Yes. `chrono` is safe to use on `wasm32-wasip2` with no special
configuration.**

`Utc::now()` goes through `std::time::SystemTime` → the WASI clock API
(`wasi:clocks`) on WASI targets. No `wasmbind` feature flag is required. No
wasm-bindgen or js-sys dependency is pulled in.

## Detail

chrono has two distinct paths for `Utc::now()` on `wasm32`:

| Target | `target_os` | Path |
|---|---|---|
| `wasm32-unknown-unknown` (browser) | `unknown` | Requires `wasmbind` feature to use `js_sys::Date`; panics at runtime without it |
| `wasm32-wasip1` / `wasm32-wasip2` (WASI) | `wasi` | Uses `std::time::SystemTime` via WASI clock API; always works |

The conditional in chrono's source that guards `Utc::now()` availability on
WASM explicitly includes WASI in the supported path:

```rust
cfg(any(
    not(target_arch = "wasm32"),  // native
    feature = "wasmbind",          // browser with js glue
    all(
        target_arch = "wasm32",
        any(target_os = "emscripten", target_os = "wasi")  // WASI always supported
    )
))
```

`wasm32-wasip2` sets `target_os = "wasi"`, so it always takes the
`std::time::SystemTime` branch.

## History

WASI support for chrono has been present since 2019
(https://github.com/chronotope/chrono/pull/365). The 2024 PR that removed
`Utc::now()` on unsupported WASM targets
(https://github.com/chronotope/chrono/pull/1567) explicitly preserved WASI
support and was motivated by catching `wasm32-unknown-unknown` panics at
compile time — not by any WASI regression.

## wasmbind feature

The `wasmbind` feature wires up `js_sys::Date` for browser WASM. It has been
disabled from default features since chrono 0.4 (merged 2024-02-29,
https://github.com/chronotope/chrono/pull/1472). On WASI targets, this
feature is irrelevant — it should not be enabled and will not be enabled by
default. Enabling it on WASI would pull in wasm-bindgen/js-sys unnecessarily
and could introduce imports that a WASI runtime (e.g. Wasmtime, Golem) does
not provide.

## Recommended dependency declaration

```toml
# In clank-transcript/Cargo.toml — no features needed for WASI compatibility
chrono = { version = "0.4", default-features = false, features = ["clock"] }
```

`default-features = false` avoids pulling in the `wasmbind` feature via
feature unification from other crates in the dependency graph. The `clock`
feature enables `Utc::now()`. This is the minimal correct configuration for
a crate targeting both native and `wasm32-wasip2`.

## Sources

- chrono PR #365 — "Add support for compilation on wasm32-wasi" (2019):
  https://github.com/chronotope/chrono/pull/365
- chrono PR #1567 — "Remove Utc::now() and Local::now() on unsupported
  platforms" (2024): https://github.com/chronotope/chrono/pull/1567
- chrono PR #1472 — "Disable wasmbind feature by default" (2024):
  https://github.com/chronotope/chrono/pull/1472
- chrono issue #1301 — "Should now error on wasm without wasmbind?" (2023):
  https://github.com/chronotope/chrono/issues/1301
