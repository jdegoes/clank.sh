---
title: "Initial Rust Workspace Structure"
date: 2026-03-06
author: agent
---

# Initial Rust Workspace Structure

This document records the realized design of the initial Rust workspace as built and verified. It supersedes the workspace structure described in the approved plan (`dev-docs/plans/approved/initial-workspace-bootstrap.md`) for future reference.

## Workspace Layout

```
/                               # repository root
├── Cargo.toml                  # [workspace] definition
├── Cargo.lock                  # committed; reproducible builds
├── rust-toolchain.toml         # stable toolchain pin
├── .cargo/
│   └── config.toml             # per-target rustflags; no global target override
├── clank-shell/
│   ├── Cargo.toml
│   └── src/
│       └── main.rs             # binary: clank
└── clank-http/
    ├── Cargo.toml
    └── src/
        └── lib.rs              # HttpClient trait + NativeHttpClient
```

## Toolchain

**`rust-toolchain.toml`**:
```toml
[toolchain]
channel    = "stable"
targets    = ["wasm32-wasip2"]
profile    = "minimal"
components = ["rustfmt", "clippy"]
```

- Channel: `stable` (Rust 1.94.0 at time of writing; `wasm32-wasip2` is Tier 2 as of 1.82).
- `wasm32-wasip2` declared as a known future target. No WASM build is attempted yet — see `dev-docs/issues/open/brush-wasm-portability.md`.
- `profile = "minimal"` avoids pulling in unnecessary components; `rustfmt` and `clippy` added explicitly.

**`.cargo/config.toml`**:
```toml
[target.wasm32-wasip2]
rustflags = ["-C", "opt-level=z", "-C", "panic=abort"]
```

No `[build] target` override. The default build target is the host native triple, ensuring `cargo build` works on any developer machine (x86_64 or aarch64, Linux or macOS) without special flags.

## Root `Cargo.toml`

```toml
[workspace]
members  = ["clank-shell", "clank-http"]
resolver = "2"

[workspace.package]
version = "0.1.0"
edition = "2021"
license = "Apache-2.0"
```

- Resolver `"2"` (the Rust 2021 edition default): required for correct feature unification across targets.
- `[workspace.package]` centralizes version, edition, and license; member crates inherit with `.workspace = true`.

## `clank-http`

### Purpose

Defines the `HttpClient` abstraction that all HTTP-using code in the workspace depends on. No call site outside this crate uses `reqwest` or any other concrete HTTP library directly.

### `Cargo.toml` dependencies

```toml
[target.'cfg(not(target_arch = "wasm32"))'.dependencies]
reqwest = { version = "0.13", features = ["json"] }
tokio   = { version = "1", features = ["rt"] }

[dev-dependencies]
tokio = { version = "1", features = ["rt", "macros"] }
```

`reqwest` and its Tokio runtime dependency are gated to non-WASM targets. This ensures `clank-http` can be compiled for `wasm32-wasip2` without pulling in native OS dependencies, even though the WASM implementation is not yet present.

### Public API surface (`src/lib.rs`)

```rust
pub struct HttpResponse { pub status: u16, pub body: Vec<u8> }
impl HttpResponse { pub fn text(&self) -> Result<&str, Utf8Error> }

pub enum HttpError { Transport(String), Status(u16) }
impl Display for HttpError
impl Error for HttpError

pub trait HttpClient: Send + Sync {
    fn get(&self, url: &str) -> impl Future<Output = Result<HttpResponse, HttpError>> + Send;
}

// Native target only:
pub struct NativeHttpClient { /* reqwest::Client */ }
impl NativeHttpClient { pub fn new() -> Self }
impl Default for NativeHttpClient
impl HttpClient for NativeHttpClient

// WASM target only (stub):
pub struct WasiHttpClient;
impl HttpClient for WasiHttpClient  // panics: todo!()
```

The `WasiHttpClient` stub exists to establish the pattern: once `brush-core` is portable to `wasm32-wasip2`, a real `wstd`-backed implementation replaces the `todo!()`. No call site needs to change.

### Design decisions

- **`trait HttpClient` uses `impl Future` in the method signature** (not `async_trait`). This is possible in stable Rust 2021 with `-> impl Future + Send` in the trait definition. It avoids a dependency on `async-trait` and the boxing overhead.
- **`Arc<dyn HttpClient + Send + Sync>` injection pattern** is not yet wired up (no shell integration yet) but the trait is `Send + Sync` so injection is possible without changes to the trait.
- **Network test marked `#[ignore]`** so it does not run in offline / CI environments by default. Run with `cargo test -- --ignored` to exercise it.

## `clank-shell`

### Purpose

The `clank` binary. Embeds `brush-core` and provides a minimal shell harness. At this stage it accepts an optional command as argv and executes it, defaulting to `echo hello`. This validates the brush-core integration and provides a functional compilation target.

### `Cargo.toml` dependencies

```toml
[dependencies]
brush-core     = "0.4"
brush-parser   = "0.3"
brush-builtins = "0.1"
tokio          = { version = "1", features = ["full"] }
```

`brush-parser` and `brush-builtins` are listed explicitly rather than relying on transitive resolution; they are direct API surfaces that `clank-shell` will use as builtins are added.

### Binary entry point (`src/main.rs`)

```rust
#[tokio::main]
async fn main() -> ExitCode

async fn run(command: &str) -> Result<u8, brush_core::Error>
```

`run` constructs a shell with `CreateOptions { interactive: false, no_profile: true, no_rc: true, no_editing: true, shell_name: Some("clank"), .. }` and executes the command via `shell.run_string(command, &params)`. The `ExecutionExitCode` return value is converted to `u8` via its `From` implementation and propagated to the process exit code via `std::process::ExitCode`.

### Shell construction options

| Option | Value | Rationale |
|---|---|---|
| `interactive` | `false` | No readline loop; single-shot command execution |
| `no_profile` | `true` | Skip `/etc/profile` and `~/.bash_profile` — not relevant at bootstrap |
| `no_rc` | `true` | Skip `~/.bashrc` / `~/.brushrc` — not relevant at bootstrap |
| `no_editing` | `true` | No reedline/crossterm — not linked at this stage |
| `shell_name` | `"clank"` | Sets `$0` |

These options will be revisited when interactive mode and configuration loading are implemented.

### `ExecutionExitCode` → `u8` conversion

`brush-core` represents exit codes as `ExecutionExitCode`, an enum with named variants (`Success`, `GeneralError`, `NotFound`, etc.) and a `Custom(u8)` catch-all. `From<ExecutionExitCode> for u8` maps back to the conventional POSIX values (0, 1, 2, 126, 127, 130, 99, custom). This conversion is called at the boundary in `run()`.

## Acceptance test results

All acceptance tests from the approved plan pass:

| Test | Result |
|---|---|
| `cargo build --workspace` | ✅ |
| `cargo run -p clank-shell` exits 0 and prints `hello` | ✅ (verified via `echo_hello_exits_zero` unit test) |
| `cargo test --workspace` | ✅ 2 passed, 1 ignored (network test) |
| `cargo clippy --workspace -- -D warnings` | ✅ no warnings |
| `cargo fmt --all --check` | ✅ no formatting issues |
| `clank-http` unit test for `NativeHttpClient` | ✅ present, marked `#[ignore]` for offline environments |

Note: the `cargo fmt` flag syntax is `--all` not `--workspace` (the `--workspace` flag is not supported by `cargo fmt`). This is a minor deviation from the plan wording; the behaviour is equivalent.

## Deviations from approved plan

None substantive. One syntactic deviation:

- The plan listed `cargo fmt --check --workspace`. The correct flag is `cargo fmt --all --check` (`--workspace` is not a valid `cargo fmt` flag). The coverage is identical.

## Open issues filed

- `dev-docs/issues/open/brush-wasm-portability.md` — documents the `brush-core` WASM compilation blocker and the three resolution paths (upstream contribution, vendored fork, feature flags).
