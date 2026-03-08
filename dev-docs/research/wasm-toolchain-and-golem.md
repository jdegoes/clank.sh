---
title: "WASM Toolchain and Golem Component Model for Rust"
date: 2026-03-06
author: agent
---

# WASM Toolchain and Golem Component Model for Rust

## wasm32-wasip2 Target

### Target triple

Use `wasm32-wasip2`. `wasm32-wasi` is the old Preview 1 name (now `wasm32-wasip1`). `wasm32-unknown-unknown` is browser/bare-metal only.

### Toolchain version

- **Rust 1.78** — `wasm32-wasip2` added as Tier 3
- **Rust 1.82** (2024-11-26) — promoted to **Tier 2** (stable, CI-tested, no nightly required)

Minimum: **Rust 1.82 stable**.

### `rust-toolchain.toml`

```toml
[toolchain]
channel    = "stable"
targets    = ["wasm32-wasip2"]
profile    = "minimal"
components = ["rustfmt", "clippy"]
```

### `.cargo/config.toml`

Do **not** set a global `[build] target` — it breaks native builds. Use per-target flags:

```toml
[target.wasm32-wasip2]
rustflags = ["-C", "opt-level=z", "-C", "panic=abort"]
```

`panic=abort` is important: stack unwinding is unavailable on WASI; abort produces smaller binaries.

Gate dependencies by target in `Cargo.toml`:

```toml
[target.'cfg(target_arch = "wasm32")'.dependencies]
wstd = "0.6"

[target.'cfg(not(target_arch = "wasm32"))'.dependencies]
tokio = { version = "1", features = ["full"] }
```

### `wstd` crate

- Version: **0.6.5** (January 2026), Bytecode Alliance
- Provides: async executor (`task::block_on`), async IO, `TcpListener`/`TcpStream`, outbound HTTP client, timers, rand — all built on WASI p2 socket/HTTP interfaces
- `std` remains available on `wasm32-wasip2` for synchronous primitives; `wstd` fills the async gap

### Async on wasm32-wasip2

**Tokio does not work on `wasm32-wasip2`** — requires epoll/kqueue/threads. Tokio WASI p2 support is an open issue (tokio#6178, unresolved as of early 2026). Recommended pattern: `wstd::task::block_on` on WASM, `#[tokio::main]` on native, with a shared `async fn run()`.

## Golem Component Model

### Tools and versions

| Tool / Crate | Version | Notes |
|---|---|---|
| `cargo-component` | **0.20.0** | Pin this exact version (Golem validated). Do NOT use `--locked` — bug in `wit-component 0.220.0` causes misaligned pointer deref; omitting `--locked` pulls patched `0.220.1`. |
| `golem-rust` | **1.11.0** | `2.0.0-dev.x` exists but is pre-release. |
| Rust target for `cargo-component` | `wasm32-wasip1` | `cargo-component` compiles to wasip1, then auto-adapts to WASI Preview 2 / Component Model. |

**`wasm-pack` is not the right tool** — it targets `wasm32-unknown-unknown` with JS bindgen.

### WIT files

Live in `wit/` at the component crate root. `cargo-component` picks them up via `[package.metadata.component]` in `Cargo.toml`.

Golem-specific host APIs (oplog, promises, worker metadata) require pulling Golem's WIT into `wit/deps/` from the `golemcloud/golem` repo.

### `Cargo.toml` for a component crate

```toml
[lib]
crate-type = ["cdylib"]   # required

[dependencies]
golem-rust = "1.11.0"     # brings wit-bindgen, wasi, etc. transitively

[package.metadata.component]
package = "example:my-component"
```

### Golem durability API (`golem-rust`)

Key primitives:

```rust
// Persistence level — controls what is written to oplog
with_persistence_level(PersistenceLevel::PersistNothing, || { ... });

// Idempotence mode
with_idempotence_mode(false, || { non_idempotent_call(); });

// Atomic regions
atomically(|| {
    debit_account();
    credit_account();
});

// Transactions with rollback
fallible_transaction(|tx| {
    let op = operation(|| charge_card(100), || refund_card(100));
    tx.execute(op)
});

// Durability flush
oplog_commit(1);
```

### Workspace compatibility

`cargo-component` works in a Cargo workspace with known caveats:
- Issue [#263](https://github.com/bytecodealliance/cargo-component/issues/263): `core`/`alloc` import resolution can fail in workspaces. Workarounds are version-specific.
- rust-analyzer requires `cargo component check` instead of plain `cargo check`.
- Build from workspace root: `cargo component build --release -p <crate-name>`

### Key gotchas

1. Pin `cargo-component@0.20.0` exactly.
2. Never `--locked` on `cargo install cargo-component`.
3. Don't add `wit-bindgen` or `wasi` as direct dependencies — `golem-rust` pins them internally.
4. `crate-type` must include `"cdylib"`.
5. `golem-examples` is archived (Feb 2025). Live templates are in `golem-cli` (`golem new`).
6. `golem-rust 2.0.0-dev.x` is pre-release — avoid.

## Implications for clank.sh

The bootstrap workspace should:
- Target **native** as the functional build for the initial issue (because `brush-core` cannot compile to WASM — see `brush-embedding-api.md`)
- Include `wasm32-wasip2` target declaration in `rust-toolchain.toml` as a stake in the ground for future work
- **Not** include `cargo-component` or Golem WIT in the initial bootstrap — Golem integration is a separate issue
- The HTTP client abstraction (`reqwest` vs `wstd`) is the right place to introduce target-conditional compilation; keep that stub in place even if unimplemented

The initial workspace bootstrap is fundamentally a native Rust project with a clear path to WASM once Brush's process abstraction layer is made portable.
