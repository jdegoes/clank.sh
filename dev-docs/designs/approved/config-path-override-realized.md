---
title: "CLANK_CONFIG Environment Variable Override — Realized Design"
date: 2026-03-06
author: agent
realized_design: true
supersedes: "dev-docs/plans/approved/config-path-override.md"
---

# CLANK_CONFIG Environment Variable Override — Realized Design

## Change summary

One function updated in two files. No new files. No schema changes.

## `clank-ask/src/config.rs` — `config_path()`

```rust
pub fn config_path() -> PathBuf {
    match std::env::var("CLANK_CONFIG") {
        Ok(val) if !val.is_empty() => PathBuf::from(val),
        _ => dirs_next::config_dir()
            .unwrap_or_else(|| PathBuf::from("~/.config"))
            .join("ask")
            .join("ask.toml"),
    }
}
```

All `AskConfig::load()` and `AskConfig::load_or_default()` calls use `config_path()`
already — no other changes needed in `clank-ask`.

## `clank-shell/src/context_process.rs` — `clank_http_config()`

The inline config path loader was updated to the same `CLANK_CONFIG`-aware logic, ensuring
`context summarize` also honours the override.

## Behaviour

| `CLANK_CONFIG` | Result |
|---|---|
| Unset | Platform default (`~/.config/ask/ask.toml` / `~/Library/…`) |
| Set to non-empty path | That path |
| Set to empty string | Platform default |

## Tests added (4)

All in `clank-ask/src/config.rs`:

- `test_config_path_uses_env_var_when_set`
- `test_config_path_uses_default_when_env_unset`
- `test_config_path_uses_default_when_env_empty`
- `test_ask_loads_config_from_env_var` — writes a temp file, sets `CLANK_CONFIG`, verifies
  the loaded config contains the expected API key

## Usage

```sh
CLANK_CONFIG=./ask.toml clank
# or
export CLANK_CONFIG=./ask.toml
```
