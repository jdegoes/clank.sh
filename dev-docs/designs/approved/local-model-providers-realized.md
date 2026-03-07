---
title: "Realized design: Local model providers (Ollama, OpenAI-compatible)"
date: 2026-03-07
author: agent
plan: "dev-docs/plans/approved/local-model-providers.md"
---

# Realized design: Local model providers

## What was built

Two new model provider implementations enabling clank to call local inference servers
without any API key.

### `OllamaProvider`

**File:** `crates/clank-ask/src/provider/ollama.rs`

Uses Ollama's native `/api/chat` endpoint (not the OpenAI-compatible endpoint). The
`"ollama/"` provider prefix is stripped before sending; the model name is passed directly
to Ollama. System prompt is sent as a `role: "system"` message, not a top-level field.
`stream: false` for all requests.

Error mapping:
- `ConnectionFailed` → informative message: "Ollama is not running at {url}. Start it with: `ollama serve`"
- HTTP 404 → "Model '{name}' not found. Pull it with: `ollama pull {name}`"
- HTTP 5xx → passthrough
- Timeout → `ProviderError::Timeout` (exit 3)

### `OpenAiCompatProvider`

**File:** `crates/clank-ask/src/provider/openai_compat.rs`

Uses the OpenAI `/v1/chat/completions` format. Covers llama.cpp (`llama-server`), LM
Studio, vLLM, LocalAI, and any future OpenAI-compatible server. The `"openai-compat/"`
prefix is stripped before sending.

Authorization header: included only when `api_key` is `Some(s)` where `s` is non-empty.
Both `None` and `Some("")` omit the header, supporting no-auth local servers.

Does not send `http-referer` or `x-openrouter-title` headers (OpenRouter-specific, not
appropriate for local servers).

### Shared wire types: `wire.rs`

**File:** `crates/clank-ask/src/provider/wire.rs`

`ChatRequest`, `ChatMessage`, `ChatResponse`, `Choice`, `AssistantMessage` — shared by
both `OpenRouterProvider` and `OpenAiCompatProvider`.

### Config extensions

**File:** `crates/clank-ask/src/config.rs`

`ProviderConfig` gained `base_url: Option<String>` with `#[serde(default)]` for
backwards compatibility. `AskConfig` gained:
- `base_url(&self, provider: &str) -> Option<&str>`
- `save(&self) -> Result<(), ConfigError>` — atomic write via temp file in same directory
- `ConfigError::Write` variant
- `DEFAULT_MODEL: &str = "anthropic/claude-sonnet-4-5"` constant

### Provider selection: `select_provider`

**File:** `crates/clank-ask/src/ask_process.rs`

Local providers handled before the key-based check. `"ollama"` defaults to
`http://localhost:11434` if no `base_url` is configured. `"openai-compat"` requires
`base_url`; if absent, returns `ProviderSelectError::MissingBaseUrl` with an actionable
hint.

### `model add` and `model list`

**File:** `crates/clank-ask/src/model_process.rs`

`model add <provider> [--url <url>] [--key <key>]` implemented for all four providers.
Selective field merge: only supplied fields are written; existing fields are preserved.
`model list` shows `base_url=<url>` for providers with a URL configured.

## Decisions made during implementation

**`OllamaProvider::new` takes a concrete `String`.** Default resolution happens at the
`select_provider` call site, consistent with `AnthropicProvider` and `OpenRouterProvider`.

**`OpenAiCompatProvider::new` takes a concrete `String` for `base_url`.** The pre-check
in `select_provider` returns an error before construction, so the constructor never
receives an invalid state. No panics.

**Base URL trailing slashes trimmed at construction.** All four provider constructors call
`trim_end_matches('/')` so URL formatting is never the source of double-slash bugs.

**`model add` merges fields, not replaces.** A `model add ollama --url http://x` on a
config that already has `api_key` for ollama preserves the key.

## Test coverage

Level 1 (unit): 6 tests for `OllamaProvider`, 8 for `OpenAiCompatProvider`, 11 for
`select_provider`, 16 for `model add`/`list`, 7 for `AskConfig` write/roundtrip.

Level 3 (scenario): 3 fixtures — `add_ollama_default.yaml`, `add_openai_compat_missing_url.yaml`,
`list_with_local_providers.yaml`.

All tests pass. `cargo clippy` and `cargo fmt --check` pass.
