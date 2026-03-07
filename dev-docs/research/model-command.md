---
title: model Command — Config Storage and WASM-Compatible Tooling
date: 2026-03-07
author: agent
---

## Purpose

Determine the right TOML library, config path resolution strategy, and
implementation approach for the `model` command, all while remaining
WASM-compatible.

---

## TOML Library

### `toml` crate (serde integration)

The `toml` crate with `serde` derive is the standard Rust TOML solution.
It is pure Rust with no `libc`, no `nix`, no OS-specific deps. Compiles to
`wasm32-wasip2`.

```toml
[dependencies]
toml = "0.8"
serde = { version = "1", features = ["derive"] }
```

This is the correct choice. No alternatives needed.

---

## Config Path Resolution

### `dirs` crate — Rejected

`dirs` depends on `libc` (for Unix) and `windows-sys`. Not WASM-compatible.

### `env_home` crate — Acceptable but unnecessary

`env_home` reads `$HOME` (Unix) or `$USERPROFILE` (Windows) with no OS API
calls. Returns `None` on WASM. Lightweight.

### `std::env::var("HOME")` — Simplest

For MVP on native, `std::env::var("HOME")` is sufficient and has zero deps.
On WASM, `$HOME` may or may not be set depending on the WASI runtime. The
shell can fall back gracefully.

**Selected: `std::env::var("HOME")` directly** — no new dependency, correct
behaviour on native, graceful on WASM.

Config path: `$HOME/.config/ask/ask.toml`

---

## Config File Schema

```toml
default_model = "anthropic/claude-sonnet-4-5"

[providers.anthropic]
api_key = "sk-ant-..."

[providers.openai]
api_key = "sk-..."
```

Rust structs:

```rust
#[derive(Debug, Serialize, Deserialize, Default)]
pub struct AskConfig {
    pub default_model: Option<String>,
    #[serde(default)]
    pub providers: HashMap<ProviderName, ProviderConfig>,
}

/// A provider name (e.g. "anthropic", "openai").
pub type ProviderName = String;

#[derive(Debug, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub api_key: String,
}
```

`HashMap<ProviderName, ProviderConfig>` uses a named type alias so the
meaning of the key is clear — per AGENTS.md code conventions.

---

## Where the `model` Command Lives

`model` is `shell-internal` scoped — intercepted in the REPL directly,
same as `context`. It does not go through Brush dispatch.

The config file read/write logic belongs in a new `clank-config` crate
(a library crate in the workspace). This keeps config logic:
- Independently testable
- Reusable by `ask` (which also needs to read the config)
- Separated from REPL dispatch logic

```
clank.sh/
├── clank/          ← REPL, ClankShell
├── clank-builtins/ ← internal commands (ls, echo, etc.)
├── clank-config/   ← NEW: AskConfig, load/save, provider resolution
├── clank-golden/   ← golden test runner
```

---

## `model` Subcommand Dispatch

The REPL intercepts lines starting with `model `:

```
"model add <provider> --key <key>"  → config.add_provider(name, key); save
"model default <model>"             → config.set_default(model); save
"model list"                        → print providers and default
```

Parsed in the REPL using simple string matching for MVP — no clap needed at
this layer since `model` is not a Brush builtin.

---

## `ask` Integration

`ask` will call `clank_config::load_config()` to get:
1. The default model name
2. The API key for the provider

This is the only consumer of `clank-config` other than the `model` command
itself.

---

## Conclusions

1. **`toml` + `serde`** for config file parsing — WASM-compatible, no extra deps
2. **`std::env::var("HOME")`** for config path — zero deps, correct on native
3. **`clank-config` crate** — owns `AskConfig`, `ProviderConfig`, load/save logic
4. **REPL intercepts `model` commands** — same pattern as `context`
5. **Named types throughout** — `ProviderName` type alias, `ProviderConfig` struct
