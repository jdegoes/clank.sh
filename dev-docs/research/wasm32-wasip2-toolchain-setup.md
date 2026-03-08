---
title: "wasm32-wasip2 Rust Toolchain Setup"
date: 2026-03-06
author: agent
---

# wasm32-wasip2 Rust Toolchain Setup

## Motivation

clank.sh targets both native and `wasm32-wasip2`. This research documents the
correct toolchain configuration, crate choices, and caveats for that dual-target
setup.

## 1. Correct Target Triple

| Target | WASI version | Notes |
|---|---|---|
| `wasm32-wasi` | Preview 1 (deprecated name) | Renamed; do not use |
| `wasm32-wasip1` | Preview 1 | Current canonical name for p1 |
| `wasm32-wasip2` | **Preview 2** | Use this |
| `wasm32-unknown-unknown` | None | Browser/bare-metal, no WASI |

The target triple for WASI Preview 2 is `wasm32-wasip2`. The old `wasm32-wasi`
name targeted Preview 1 and has since been renamed to `wasm32-wasip1`.

## 2. Minimum Toolchain Version

- **Rust 1.78** (2024-05-02): `wasm32-wasip2` added as Tier 3.
- **Rust 1.82** (2024-11-26): promoted to **Tier 2** — CI-tested, stable,
  ready for production use.

Use `stable` channel at `1.82` or later. No nightly required.

```toml
# rust-toolchain.toml
[toolchain]
channel  = "stable"
targets  = ["wasm32-wasip2"]
profile  = "minimal"
components = ["rustfmt", "clippy"]
```

## 3. Dual-Target Workspace Configuration

### rust-toolchain.toml

List `wasm32-wasip2` in `targets` so rustup installs it automatically for
every developer and CI run that checks out the repository.

```toml
[toolchain]
channel  = "stable"
targets  = ["wasm32-wasip2"]
profile  = "minimal"
components = ["rustfmt", "clippy"]
```

### .cargo/config.toml

Do **not** set a global `[build] target` — this would break native builds.
Instead, use per-target `rustflags` and rely on explicit `--target` flags
(or per-crate `[package.metadata]`) when building for wasm.

```toml
# .cargo/config.toml

[target.wasm32-wasip2]
rustflags = ["-C", "opt-level=z", "-C", "panic=abort"]

[target.x86_64-unknown-linux-gnu]
rustflags = ["-C", "opt-level=3"]

[target.aarch64-apple-darwin]
rustflags = ["-C", "opt-level=3"]
```

`panic=abort` is strongly recommended for wasm32-wasip2: stack-unwinding
requires `libunwind` which is not available on WASI, and `abort` produces
smaller binaries.

Build commands:

```bash
# native
cargo build

# wasm
cargo build --target wasm32-wasip2
```

### Workspace-level cfg

Use `[target.'cfg(target_arch = "wasm32")']` in `Cargo.toml` dependency
sections to gate wasm-only deps:

```toml
[target.'cfg(target_arch = "wasm32")'.dependencies]
wstd = "0.6"

[target.'cfg(not(target_arch = "wasm32"))'.dependencies]
reqwest = { version = "0.12", features = ["json"] }
tokio   = { version = "1", features = ["full"] }
```

## 4. The `wstd` Crate

- **Current version:** 0.6.5 (January 2026)
- **Repository:** https://github.com/bytecodealliance/wstd
- **Maintainer:** Bytecode Alliance (yoshuawuyts et al.)

### What it provides

`wstd` is an async standard library designed specifically for WASI 0.2
(Preview 2) and the WebAssembly Component Model. Key modules:

| Module | Contents |
|---|---|
| `wstd::future` | Future utilities |
| `wstd::io` | Async IO traits and helpers |
| `wstd::net` | `TcpListener`, `TcpStream` (WASI socket interfaces) |
| `wstd::http` | Outbound HTTP client built on WASI HTTP |
| `wstd::time` | Timers (`sleep`, `Instant`) |
| `wstd::task` | `block_on` entry-point executor |
| `wstd::rand` | Random number generation via WASI |

### Comparison to `std` on WASI p2

`std` compiles on `wasm32-wasip2` and covers synchronous IO, filesystem,
environment, process, etc. What it does **not** provide:

- An async executor
- Async networking types (no `tokio::net` equivalent)
- WASI-specific component-model interfaces

`wstd` fills the async gap. The typical pattern is to use `std` for
synchronous primitives and `wstd` for async IO and the executor.

### When to use `wstd` vs `std`

- Use `std` (always available) for `String`, `Vec`, `HashMap`, `File`,
  synchronous process/environment APIs, etc.
- Use `wstd` when you need async IO, networking, HTTP, or timers on the
  WASM target.
- Do **not** use Tokio on `wasm32-wasip2` — see section 5.

### Example

```rust
use wstd::{io, net::TcpListener, task};

fn main() -> io::Result<()> {
    task::block_on(async {
        let listener = TcpListener::bind("127.0.0.1:8080").await?;
        println!("listening on {}", listener.local_addr()?);
        Ok(())
    })
}
```

## 5. Async on wasm32-wasip2

### Tokio — not compatible

Tokio requires OS threads, epoll/kqueue, and `mio`. None of these exist on
WASI p2. Plain `tokio` does **not** compile for `wasm32-wasip2`.

- `tokio_wasi` (v1.25.2) is a fork that patched Tokio for `wasm32-wasi`
  (Preview 1). Its maintenance status for wasip2 is unclear and it is not
  an official Tokio release.
- `tokio_with_wasm` targets `wasm32-unknown-unknown` (browser), not WASI.
- There is an open Tokio issue (#6178) tracking proper WASI support; as of
  early 2026 it is unresolved.

**Do not use Tokio on the wasm32-wasip2 target.**

### Recommended async approach

Use `wstd::task::block_on` as the executor entry-point. It wraps the WASI
poll-loop. Futures written against standard `async`/`await` and `std::future`
traits work correctly inside it.

### Conditional compilation pattern

```rust
#[cfg(not(target_arch = "wasm32"))]
#[tokio::main]
async fn main() { run().await; }

#[cfg(target_arch = "wasm32")]
fn main() {
    wstd::task::block_on(run());
}

async fn run() { /* shared logic */ }
```

### `async-std` and `smol`

Neither has explicit first-party support for `wasm32-wasip2` as of early
2026. They may compile with some feature flags but are untested in the WASI
component model context. Not recommended.

## 6. `cargo-component`

- **Repository:** https://github.com/bytecodealliance/cargo-component
- **Current version:** ~0.21.x (check releases for latest)
- **Maintainer:** Bytecode Alliance

### What it does

`cargo-component` is a Cargo subcommand for building **WebAssembly
Components** — the structured, interface-typed units defined by the
WebAssembly Component Model. It:

- Generates Rust bindings from WIT (WebAssembly Interface Types) files
- Wraps the compiled `.wasm` module in a component envelope
- Manages component metadata and world declarations
- Supports `cargo component publish` to OCI/component registries

### Is it required for `wasm32-wasip2`?

**No.** Plain `cargo build --target wasm32-wasip2` produces a valid WASI p2
module. You do not need `cargo-component` to run code under Wasmtime or
other WASI runtimes.

`cargo-component` is required (or strongly beneficial) when:
- Your output must conform to a specific WIT world (interface contract)
- You need to compose multiple components
- You are publishing to a component registry

### Module vs. Component distinction

| | WASI Module | WASI Component |
|---|---|---|
| Format | Plain `.wasm` | `.wasm` with component-model wrapping |
| Interfaces | WASI system calls (imports) | Typed WIT worlds (imports + exports) |
| Tooling | `cargo build` | `cargo-component` or `wasm-tools component new` |
| Composition | Not composable | Composable via `wasm-tools compose` |
| Use case | Simple CLI / server apps | Plugin systems, cross-language linking |

For clank.sh, which is a WASM-based shell rather than a plugin host, a plain
module produced by `cargo build --target wasm32-wasip2` is sufficient.

### Install and use (if needed)

```bash
cargo install cargo-component --locked

# scaffold a new component project
cargo component new --lib my_component

# build
cargo component build --target wasm32-wasip2
```

Note: `cargo-component` has a known issue (#364) where it does not fully
respect `.cargo/config.toml` for target overrides. Specify `--target`
explicitly on the command line.

## Summary

| Concern | Decision |
|---|---|
| Target triple | `wasm32-wasip2` |
| Min toolchain | Rust 1.82 stable |
| Async executor (wasm) | `wstd::task::block_on` |
| Async runtime (native) | Tokio |
| Tokio on wasm | Not supported — avoid |
| `cargo-component` | Not required for basic wasm output |
| `std` on wasm | Available; use for sync primitives |
| `wstd` version | 0.6.5 |

## References

- https://blog.rust-lang.org/2024/04/09/updates-to-rusts-wasi-targets
- https://blog.rust-lang.org/2024/11/26/wasip2-tier-2
- https://doc.rust-lang.org/nightly/rustc/platform-support/wasm32-wasip2.html
- https://crates.io/crates/wstd
- https://github.com/bytecodealliance/wstd
- https://github.com/bytecodealliance/cargo-component
- https://github.com/tokio-rs/tokio/issues/6178
