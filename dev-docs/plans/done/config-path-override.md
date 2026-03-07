---
title: "Plan: CLANK_CONFIG environment variable override"
date: 2026-03-06
author: agent
issue: "dev-docs/issues/open/config-path-override.md"
research: []
designs:
  - "dev-docs/designs/approved/phase-1-transcript-and-ask-realized.md"
---

# Plan: CLANK_CONFIG Environment Variable Override

## Originating Issue

`dev-docs/issues/open/config-path-override.md` — the config file path is hardcoded to
the platform default; no way to use a project-local or CI-injected config.

## Developer Feedback

None required — scope and approach are unambiguous.

## Approach

Add a single `config_path()` helper that checks `CLANK_CONFIG` first, then falls back to
the platform default. All call sites — `AskConfig::load()`, `AskConfig::load_or_default()`,
and the inline config loader in `context_process.rs` — already call `config_path()` or
equivalent. One change covers everything.

### Behaviour

| Condition | Result |
|---|---|
| `CLANK_CONFIG` unset | Use platform default (`dirs-next::config_dir()/ask/ask.toml`) |
| `CLANK_CONFIG=/path/to/file`, file exists | Load from that path |
| `CLANK_CONFIG=/path/to/file`, file absent | `ConfigError::NotFound { path }` — same error as missing default config |
| `CLANK_CONFIG` set to empty string | Treat as unset; use platform default |

---

## Tasks

- [ ] **`clank-ask/src/config.rs`** — update `config_path()`
  - [ ] Check `std::env::var("CLANK_CONFIG")` first; if set and non-empty, use that path
  - [ ] Otherwise fall back to existing `dirs-next` logic
  - [ ] Unit tests:
    - `test_config_path_uses_env_var_when_set`
    - `test_config_path_uses_default_when_env_unset`
    - `test_config_path_uses_default_when_env_empty`

- [ ] **`clank-shell/src/context_process.rs`** — update inline `clank_http_config()`
  - [ ] Replace the hardcoded `dirs-next` path with a call to the same env-var-aware
        helper (or inline the same logic)
  - [ ] Unit test: `context summarize` picks up config from `CLANK_CONFIG`

- [ ] **`TUTORIAL.md`** — add a note in section 5 explaining `CLANK_CONFIG`

- [ ] **Final quality gate**
  - [ ] `cargo test` — all tests pass
  - [ ] `cargo clippy --all-targets -- -D warnings` — clean
  - [ ] `cargo fmt --check` — clean

---

## Acceptance Tests

| # | Test | Location | Assertion |
|---|---|---|---|
| 1 | `test_config_path_uses_env_var_when_set` | `config.rs` | `config_path()` returns the `CLANK_CONFIG` value when set |
| 2 | `test_config_path_uses_default_when_env_unset` | `config.rs` | `config_path()` returns platform default when `CLANK_CONFIG` not set |
| 3 | `test_config_path_uses_default_when_env_empty` | `config.rs` | `config_path()` returns platform default when `CLANK_CONFIG=""` |
| 4 | `test_ask_loads_config_from_env_var` | `ask_process.rs` | `run_ask` with `CLANK_CONFIG` pointing at a temp file picks up the API key from it |
