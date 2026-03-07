---
title: Implement model Command and clank-config Crate
date: 2026-03-07
author: agent
issue: dev-docs/issues/open/model-command.md
research:
  - dev-docs/research/model-command.md
designs: []
---

## Summary

Introduce a `clank-config` library crate that owns the `AskConfig` data
structure, config file load/save logic, and provider resolution. Intercept
`model add`, `model default`, and `model list` in the REPL directly
(shell-internal scope). Both the `model` command and `ask` read config via
`clank-config`.

## Developer Feedback

- `model` is `shell-internal` — intercepted in REPL directly, same as `context`
- `clank-config` is a separate crate so `ask` can reuse config loading
- Named types throughout: `ProviderName` type alias, `ProviderConfig` struct
- WASM-compatible: `toml` + `serde`, `std::env::var("HOME")` for path resolution
- MVP scope: `model add`, `model default`, `model list` only

## New Crate: `clank-config`

```
clank.sh/
├── Cargo.toml            ← add clank-config to members
└── clank-config/
    ├── Cargo.toml        ← deps: toml, serde
    └── src/
        └── lib.rs        ← AskConfig, ProviderConfig, load(), save()
```

### Types

```rust
/// The name of a model provider (e.g. "anthropic", "openai").
pub type ProviderName = String;

/// Configuration for a single model provider.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ProviderConfig {
    pub api_key: String,
}

/// The full ask configuration, stored at ~/.config/ask/ask.toml.
#[derive(Debug, Serialize, Deserialize, Default)]
pub struct AskConfig {
    /// The default model to use (e.g. "anthropic/claude-sonnet-4-5").
    pub default_model: Option<String>,
    /// Registered providers and their API keys.
    #[serde(default)]
    pub providers: HashMap<ProviderName, ProviderConfig>,
}
```

### Public API

```rust
/// Returns the path to ~/.config/ask/ask.toml.
/// Returns None if $HOME is not set (e.g. on WASM without HOME env var).
pub fn config_path() -> Option<PathBuf>

/// Load AskConfig from disk. Returns Default if file does not exist.
pub fn load_config() -> Result<AskConfig, ConfigError>

/// Save AskConfig to disk. Creates parent directories if needed.
pub fn save_config(config: &AskConfig) -> Result<(), ConfigError>
```

### Error type

```rust
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("config path unavailable: $HOME is not set")]
    NoHomePath,
    #[error("failed to read config: {0}")]
    Io(#[from] std::io::Error),
    #[error("failed to parse config: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("failed to serialize config: {0}")]
    Serialize(#[from] toml::ser::Error),
}
```

## REPL Dispatch

`model` commands are intercepted in `run_repl()` before Brush dispatch,
matching on the trimmed input string:

```
"model list"                          → model_list(&config)
s if s.starts_with("model default ")  → model_set_default(s, &mut config)
s if s.starts_with("model add ")      → model_add(s, &mut config)
_                                     → run_command(trimmed).await
```

Each handler loads the config, mutates it, and saves it back. Config is
loaded fresh on each `model` command — no in-memory caching needed for MVP.

## `model list` Output Format

```
Providers:
  anthropic  (api_key: sk-ant-...*****)
  openai     (api_key: sk-...*****)

Default model: anthropic/claude-sonnet-4-5
```

API keys are partially redacted — last 5 chars only, rest replaced with `*`.

## `ClankShell` Changes

`run_repl()` gains `model` command interception. `ClankShell` itself does not
change — config is a separate concern.

## Acceptance Tests

1. `cargo test` passes — all 75 existing tests still green.
2. `model add anthropic --key sk-test-key` writes the key to `~/.config/ask/ask.toml`.
3. `model default anthropic/claude-sonnet-4-5` sets the default model.
4. `model list` prints configured providers and the current default.
5. Loading config when file does not exist returns `AskConfig::default()` without error.
6. Unit tests in `clank-config` cover: `load_config` round-trip, `save_config`, `config_path`, missing home dir.
7. `cargo clippy --all-targets -- -D warnings` passes.

## Tasks

- [ ] Add `clank-config` to workspace `Cargo.toml` members
- [ ] Create `clank-config/Cargo.toml` with `toml`, `serde`, `thiserror`
- [ ] Implement `ProviderName`, `ProviderConfig`, `AskConfig`, `ConfigError` in `clank-config/src/lib.rs`
- [ ] Implement `config_path()`, `load_config()`, `save_config()` in `clank-config/src/lib.rs`
- [ ] Add unit tests for config round-trip, missing file, missing home dir
- [ ] Add `clank-config` as dependency of `clank`
- [ ] Intercept `model add`, `model default`, `model list` in `run_repl()` in `clank/src/lib.rs`
- [ ] Implement `model_add`, `model_set_default`, `model_list` as private functions in `clank/src/lib.rs`
- [ ] Add integration tests in `clank/tests/repl.rs` for `model list` output
- [ ] Verify all acceptance tests pass
