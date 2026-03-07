---
title: "OpenRouter Provider — Realized Design"
date: 2026-03-06
author: agent
realized_design: true
supersedes: "dev-docs/plans/approved/openrouter-provider.md"
---

# OpenRouter Provider — Realized Design

## Overview

This document records the OpenRouter provider implementation as built. It supersedes the
plan as the reference for future work.

---

## New file: `clank-ask/src/provider/openrouter.rs`

`OpenRouterProvider` implements `ModelProvider` using OpenRouter's OpenAI-compatible chat
completions API.

### Wire format

```
POST https://openrouter.ai/api/v1/chat/completions
Authorization: Bearer <api_key>
Content-Type: application/json
HTTP-Referer: https://clank.sh
X-OpenRouter-Title: clank.sh

{
  "model": "anthropic/claude-sonnet-4-5",   // full string, no prefix stripping
  "max_tokens": 4096,
  "system": "<system prompt>",
  "messages": [{"role": "user", "content": "<prompt>"}]
}
```

Response: `choices[0].message.content` (OpenAI format, distinct from Anthropic's
`content[0].text`).

### Error mapping

| HTTP status | `ProviderError` | Exit code |
|---|---|---|
| 401, 402 | `NotConfigured` | 1 |
| 429 | `Timeout` | 3 |
| 502, 503, other non-2xx | `RemoteCallFailed` | 4 |
| Connection failure | `RemoteCallFailed` | 4 |
| `HttpError::Timeout` | `Timeout` | 3 |

### `base_url` override

`OpenRouterProvider::with_base_url(api_key, http, base_url)` allows tests to inject a
custom URL, matching the pattern established by `AnthropicProvider`.

---

## Updated: `clank-ask/src/provider/mod.rs`

`pub mod openrouter;` added alongside `pub mod anthropic;`.

---

## Updated: `clank-ask/src/ask_process.rs`

### New function: `select_provider`

```rust
fn select_provider(
    model: &str,
    config: &AskConfig,
    http: Arc<dyn HttpClient>,
) -> Result<Box<dyn ModelProvider>, String>
```

Selection logic:

1. Extract provider prefix from model name (e.g. `"anthropic"` from
   `"anthropic/claude-sonnet-4-5"`).
2. If `config.api_key(provider_name)` is `Some`:
   - `"anthropic"` → `AnthropicProvider` (direct)
   - Any other prefix with a direct key → `OpenRouterProvider` (provider has a key
     configured; route through OpenRouter since it supports all `provider/model` strings)
3. Else if `config.api_key("openrouter")` is `Some` → `OpenRouterProvider`
4. Else → error message mentioning both options

The direct Anthropic key takes precedence over an OpenRouter key for Anthropic models. This
preserves backward compatibility — existing configs are unaffected.

### Provider call

`provider.as_ref().complete(request).await` — called through the `Box<dyn ModelProvider>`
trait object. No change to request construction, response handling, exit codes, or `--json`
contract.

---

## Config — no schema changes

`AskConfig.providers` is `HashMap<String, ProviderConfig>`. OpenRouter is just a new key:

```toml
[providers.openrouter]
api_key = "sk-or-..."
```

`AskConfig::resolve_model` and `api_key` methods are unchanged.

---

## Tests added

### `provider/openrouter.rs` (unit)

| Test | What it verifies |
|---|---|
| `test_openrouter_mock_success` | Response text extracted from `choices[0].message.content` |
| `test_openrouter_builds_correct_request` | Full model string; `Authorization: Bearer`; app headers; correct URL |
| `test_openrouter_mock_timeout` | `HttpError::Timeout` → `ProviderError::Timeout`, exit 3 |
| `test_openrouter_rate_limit_maps_to_timeout` | HTTP 429 → `ProviderError::Timeout`, exit 3 |
| `test_openrouter_mock_http_error` | HTTP 502 → `ProviderError::RemoteCallFailed`, exit 4 |
| `test_openrouter_mock_unauthorized` | HTTP 401 → `ProviderError::NotConfigured`, exit 1 |

### `ask_process.rs` (unit)

| Test | What it verifies |
|---|---|
| `test_provider_selection_anthropic_direct` | Anthropic key → direct Anthropic endpoint |
| `test_provider_selection_openrouter_fallback` | No anthropic key, openrouter key → OpenRouter endpoint |
| `test_provider_selection_direct_takes_precedence` | Both keys → direct anthropic wins |
| `test_provider_selection_no_key_exits_1` | No keys → exit 1, message mentions both options |
| `test_provider_selection_openrouter_with_anthropic_model` | OpenRouter receives full `"anthropic/claude-..."` string |

All 37 pre-existing `clank-ask` tests continue to pass.

---

## Documentation updates

- `TUTORIAL.md` section 5 expanded with Option A (Anthropic direct) and Option B (OpenRouter)
  including model name examples and link to OpenRouter model list.
- `OVERVIEW.md` "What has been built" updated to mention OpenRouter support.
