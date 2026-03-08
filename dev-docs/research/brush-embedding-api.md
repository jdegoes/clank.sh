---
title: "Brush Shell Embedding API"
date: 2026-03-06
author: agent
---

# Brush Shell Embedding API

## Crate Versions (as of 2025-11-17)

| Crate | Version | MSRV |
|---|---|---|
| `brush-core` | 0.4.0 | 1.87.0 |
| `brush-parser` | 0.3.0 | 1.87.0 |
| `brush-builtins` | 0.1.0 | 1.87.0 |
| `brush-interactive` | 0.3.0 | 1.87.0 |

## Embedding API

### Shell construction

Entry point is `Shell::builder()`, using the `bon` builder macro:

```rust
// Default extensions
let shell = Shell::builder().build().await?;

// Custom extensions
let shell = Shell::builder_with_extensions::<MyExtensions>().build().await?;
```

`ShellBuilder::build()` is `async`. It constructs the shell and loads profile/rc files.

### Custom builtin registration

Three factory functions in `brush_core::builtins`:

```rust
// For clap::Parser + Command impls (most common)
pub fn builtin<B: Command + Send + Sync, SE: ShellExtensions>() -> Registration<SE>

// For simple non-clap commands
pub fn simple_builtin<B: SimpleCommand + Send + Sync, SE: ShellExtensions>() -> Registration<SE>

// For commands that take parsed declaration args (like `declare`, `local`)
pub fn decl_builtin<B: DeclarationCommand + Send + Sync, SE: ShellExtensions>() -> Registration<SE>
```

Add via builder:
```rust
Shell::builder()
    .builtin("my-cmd", brush_core::builtins::builtin::<MyCmd, _>())
    .build()
    .await?
```

The `Command` trait:
```rust
pub trait Command: clap::Parser {
    type Error: BuiltinError + 'static;
    fn execute<SE: ShellExtensions>(
        &self,
        context: ExecutionContext<'_, SE>,
    ) -> impl Future<Output = Result<ExecutionResult, Self::Error>> + Send;
}
```

`ExecutionContext<'_, SE>` provides mutable access to `Shell<SE>`, `command_name`, stdin/stdout/stderr, and parameters.

### ShellExtensions

Static compile-time extension point:

```rust
pub trait ShellExtensions: Clone + Default + Send + Sync + 'static {
    type ErrorFormatter: ErrorFormatter;
}
```

All dispatch is monomorphic (no runtime vtable). The only extension point is `ErrorFormatter`.

### `brush-builtins` feature flags

Every standard builtin can be individually enabled or disabled via Cargo features. This allows embedding with a minimal builtin set.

### Async / Tokio dependency

`brush-core` **requires Tokio**. It uses `tokio::sync::Mutex`, `tokio::select!` for signal handling, and `BoxFuture` for async dispatch. All shell execution is async and must run inside a Tokio runtime. Custom builtins must be `Send`.

## Critical Blocker: No WASM/WASI Support

`brush-core` **does not support `wasm32-wasip2` and has no path to doing so without major rewrites**.

Blockers:

1. **`nix ^0.30.1` dependency** — provides Unix POSIX APIs (signals, process groups, `fork`/`exec`, `ioctl`). Does not support `wasm32` targets.
2. **Tokio process/signal features** — hard Unix-only: `tokio::process::Command`, `tokio::signal`, SIGTSTP/SIGCHLD/SIGINT handlers in a `tokio::select!` loop.
3. **`procfs` in `brush-builtins`** — requires Linux `/proc` filesystem.
4. **`uzers` / `whoami` / `hostname`** — user/host info crates requiring native OS APIs.
5. **No open issues or planned work** on WASM support — not on the roadmap.

The `sys/` platform abstraction layer refactoring (PR #735, merged Nov 2025) targets Windows support, not WASM.

## No Pluggable Process Spawner

There is **no extensible process spawner interface** in `brush-core`. External commands always go through `std::process::Command` / Tokio process APIs. The clank.sh design calls for replacing the entire process execution layer with an internal async process abstraction — this cannot be done by implementing an interface; it would require forking or patching the crate.

## Implications for clank.sh

The clank.sh README states that Brush is the foundation and that clank.sh "replaces Brush's Unix process-spawning layer entirely with an internal async process trait." The research shows:

- This is architecturally sound for the **native target**: Brush can be embedded with custom builtins, and the `sys/` abstraction layer is being refactored.
- For the **WASM target**: Brush cannot be compiled to `wasm32-wasip2` at all today. The initial workspace bootstrap must target native only, with WASM portability deferred to a later issue after a process abstraction interface is designed and contributed upstream (or a fork/vendored copy is used).

The workspace bootstrap plan must reflect this: the WASM build target is aspirational for initial scaffolding — the native build is the deliverable for the first issue.
