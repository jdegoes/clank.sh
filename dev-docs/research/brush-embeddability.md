---
title: Brush Crate Structure and Embeddability for Native Shell Scaffold
date: 2026-03-07
author: agent
---

## Purpose

Determine what Brush crates are needed, what their public APIs look like, and whether `brush-core` can be embedded to produce a minimal interactive REPL for native Rust without pulling in the full `brush-shell` binary.

---

## Crate Inventory

Brush (https://github.com/reubeno/brush) decomposes into four independently usable crates:

| Crate | Version | Role |
|---|---|---|
| `brush-parser` | 0.3.0 | POSIX/bash tokenizer and AST parser |
| `brush-core` | 0.4.0 | Embeddable shell interpreter; `Shell` struct, builtin registration, execution |
| `brush-builtins` | 0.1.0 | Default builtin set; registered optionally via `ShellBuilderExt` trait |
| `brush-interactive` | 0.3.0 | Interactive REPL layer (readline via `reedline`, basic, or minimal backends) |

`brush-shell` is the standalone binary crate that wires all of the above together. clank.sh does **not** use `brush-shell` — it embeds the sub-crates directly.

---

## `brush-core` Public API

### Creating a Shell

```rust
// Via CreateOptions (all fields have defaults via Default impl)
let options = brush_core::CreateOptions {
    interactive: true,
    shell_name: Some("clank".to_string()),
    no_profile: true,   // skip sourcing /etc/profile for now
    no_rc: true,        // skip sourcing ~/.bashrc for now
    ..Default::default()
};
let shell = brush_core::Shell::new(options).await?;
```

Alternatively, `Shell::builder()` returns a `ShellBuilder` for fluent construction.

### Executing Commands

```rust
let params = shell.default_exec_params();
let result = shell.run_string("echo hello", &params).await?;
// result.exit_code is u8
```

### Registering Custom Builtins

```rust
shell.register_builtin("my-cmd", brush_core::builtins::Registration { ... });
```

The `brush-builtins` crate exposes a `ShellBuilderExt` trait that registers all default builtins in one call.

---

## `brush-interactive` Public API

`brush-interactive` provides the `InteractiveShell` trait and three concrete input backends:

| Backend | Feature flag | Notes |
|---|---|---|
| `ReedlineInputBackend` | `reedline` | Full readline with history, completion, highlighting |
| `BasicInputBackend` | `basic` | Simple line-by-line stdin |
| `MinimalInputBackend` | `minimal` | Bare minimum; no completion or history |

For the hello-world scaffold, `BasicInputBackend` or `MinimalInputBackend` is sufficient — no readline dependency required.

The `brush-interactive` crate **also depends on `nix`** (for signal handling on Unix). This is important: it means `brush-interactive` is also not safe for `wasm32-wasip2` as-is. For the native scaffold this is fine; for WASM we will need to write our own input loop against `brush-core` directly.

### Using the Interactive Layer

From `brush-shell`'s `entry.rs`, the pattern is:
1. Build a `Shell` via `CreateOptions`
2. Wrap it in `Arc<Mutex<Shell>>`
3. Instantiate an input backend
4. Call `run_in_shell(&shell, args, &mut input_backend, &ui_options).await`

For clank.sh's hello-world, we can write a simpler loop directly:

```rust
loop {
    // print prompt
    // read line from stdin
    // shell.run_string(line, &params).await
}
```

This avoids `brush-interactive` entirely for the scaffold, keeping dependencies minimal.

---

## `nix` Crate Dependency — Key Finding

`brush-core` v0.4.0 has `nix = "^0.30.1"` as a **normal** (non-optional) dependency. This is used internally for Unix process operations (job control, signal handling, pty management).

**Consequence:** `brush-core` as published cannot compile to `wasm32-wasip2` without modification. This is the known seam identified in the README (`README.md:155`): "Brush's use of the `nix` crate for Unix process operations. Since clank replaces the entire process execution layer, `nix` usage is excluded at that boundary via conditional compilation."

**For the native hello-world scaffold this is not a problem** — `nix` compiles fine on macOS and Linux. The WASM target seam is a future concern.

Possible future approaches for WASM (upstream contribution is explicitly ruled out by the developer):
- Fork `brush-core` and apply `#[cfg(not(target_arch = "wasm32"))]` guards around `nix` usage as a path-dependency
- Use a git patch/overlay that makes `nix` usage conditional without modifying published crates
- Replace the affected process layer entirely within clank.sh, leaving `brush-core` untouched

This is out of scope for the hello-world scaffold.

---

## Minimum Viable Dependencies for Native Scaffold

```toml
[dependencies]
brush-core = "0.4.0"
brush-builtins = "0.1.0"
tokio = { version = "1", features = ["full"] }
```

`brush-core` requires `tokio` because `Shell::new` and `Shell::run_string` are `async`. The `tokio::main` macro on `main()` is sufficient.

`brush-interactive` is **not required** for hello-world — a manual `stdin` read loop works fine.

---

## Conclusions

1. **`brush-core` is embeddable** — `Shell::new(CreateOptions { ... }).await` is the entry point. No friction.
2. **A basic REPL loop needs only `brush-core` + `brush-builtins` + `tokio`** — ~3 dependencies.
3. **`nix` blocks WASM but not native** — the hello-world scaffold is unaffected. WASM seam is a future task.
4. **`brush-interactive` adds readline but also adds `nix`** — we skip it for now and write a simple stdin loop.
5. **The `Shell` struct is generic over a `ShellExtensions` type parameter** — this is the extension point for clank.sh's custom process dispatch. For hello-world, the default extensions implementation suffices.
