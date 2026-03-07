---
title: model Command and clank-config Crate — Realized Design
date: 2026-03-07
author: agent
---

## Overview

This document records the `model` command and `clank-config` crate as actually
built. It supersedes any prior approved design for this area (none existed).

---

## What Was Built

A new `clank-config` library crate owning the `AskConfig` data structure,
config file load/save logic, and provider resolution. Three `model` subcommands
(`add`, `default`, `list`) intercepted in the REPL directly (shell-internal
scope). Config stored at `~/.config/ask/ask.toml` in TOML format.

---

## Workspace Structure

```
clank.sh/
├── Cargo.toml              ← clank-config added to members
└── clank-config/
    ├── Cargo.toml          ← toml, serde, thiserror; tempfile (dev)
    └── src/
        └── lib.rs          ← all types, config_path, load_config, save_config
```

---

## Types (`clank-config/src/lib.rs`)

### `ProviderName`

```rust
/// The name of a model provider (e.g. "anthropic", "openai").
pub type ProviderName = String;
```

A named type alias — per AGENTS.md code conventions, the meaning of the
`String` is explicit.

### `ProviderConfig`

```rust
pub struct ProviderConfig {
    pub api_key: String,
}
```

### `AskConfig`

```rust
pub struct AskConfig {
    pub default_model: Option<String>,
    pub providers: HashMap<ProviderName, ProviderConfig>,
}
```

Methods:
- `add_provider(name, api_key)` — insert or overwrite a provider
- `set_default_model(model)` — set the default model string
- `api_key_for_model(model) -> Option<&str>` — resolve API key for a model:
  1. Try `provider/model` prefix split
  2. Try model name as provider name
  3. Fall back to sole configured provider if only one exists

### `ConfigError`

```rust
pub enum ConfigError {
    NoHomePath,          // $HOME not set
    Io(std::io::Error),
    Parse(toml::de::Error),
    Serialize(toml::ser::Error),
}
```

---

## Config File

Path: `$HOME/.config/ask/ask.toml`  
Resolved via: `std::env::var("HOME")` — zero deps, WASM-compatible.

Format:

```toml
default_model = "anthropic/claude-sonnet-4-5"

[providers.anthropic]
api_key = "sk-ant-..."
```

---

## Public API

```rust
pub fn config_path() -> Option<PathBuf>
pub fn load_config() -> Result<AskConfig, ConfigError>
pub fn save_config(config: &AskConfig) -> Result<(), ConfigError>
```

`load_config()` returns `AskConfig::default()` if the file does not exist —
not an error. This allows `ask` and `model list` to work without panicking
on a fresh install.

---

## REPL Dispatch

Three `model` commands intercepted in `run_repl()` before Brush dispatch,
consistent with the `context` and `exit` patterns:

| Input | Handler | Behaviour |
|---|---|---|
| `model list` | `model_list()` | Load config, print providers + default |
| `model add <p> --key <k>` | `model_add(s)` | Load, add provider, save |
| `model default <m>` | `model_set_default(s)` | Load, set default, save |

Each handler loads config fresh from disk, mutates it, and saves — no
in-memory config caching. This keeps the REPL stateless with respect to
config and means external changes to the file are always picked up.

---

## `model list` Output

```
Providers:
  anthropic  (api_key: *************-key)

Default model: anthropic/claude-sonnet-4-5
```

API keys are partially redacted via `redact_key()`: last 5 characters shown,
rest replaced with `*`. This prevents accidental key leakage in terminal
recordings or screenshots.

---

## `ask` Integration Point

`ask` will call `clank_config::load_config()` to retrieve:
1. `config.default_model` — which model to invoke
2. `config.api_key_for_model(model)` — the API key for that provider

`clank-config` is the single source of truth for both.

---

## Test Coverage

| Layer | Count | What |
|---|---|---|
| Unit — `clank-config` | 7 | config_path, load missing file, round-trip, api_key_for_model (3 cases), add_provider overwrite |
| Integration — `repl.rs` | 3 | model list (no providers), model add then list, model default then list |

Tests use `tempfile::tempdir()` to override `$HOME` — ensures tests never
touch the developer's real `~/.config/ask/ask.toml`.

**Total: 85 tests, all passing. Clippy clean.**

---

## Deviations from the Approved Plan

None. All tasks completed as specified.
