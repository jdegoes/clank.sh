---
title: "Plan: Add OpenRouter provider"
date: 2026-03-06
author: agent
issue: "dev-docs/issues/open/openrouter-provider.md"
research:
  - "dev-docs/research/openrouter-api.md"
designs:
  - "dev-docs/designs/approved/phase-1-transcript-and-ask-realized.md"
---

# Plan: Add OpenRouter Provider

## Originating Issue

`dev-docs/issues/open/openrouter-provider.md` — clank.sh supports only Anthropic directly;
OpenRouter and all non-Anthropic models are inaccessible.

## Research Consulted

`dev-docs/research/openrouter-api.md` — full API wire format, auth, request/response schema,
error codes, and comparison with the existing Anthropic implementation.

## Developer Feedback

None required — scope and approach are clear from research. No significant design decisions
outstanding.

## Approach

Add `OpenRouterProvider` implementing the `ModelProvider` trait. Update provider selection
in `run_ask` to route based on the model name prefix. No changes outside `clank-ask`.

### Wire format

OpenRouter uses the OpenAI chat completions format:

```
POST https://openrouter.ai/api/v1/chat/completions
Authorization: Bearer <api_key>
Content-Type: application/json

{
  "model": "anthropic/claude-sonnet-4-5",
  "max_tokens": 4096,
  "system": "<system prompt>",
  "messages": [{"role": "user", "content": "<prompt>"}]
}
```

Response:
```json
{
  "choices": [{"message": {"role": "assistant", "content": "<text>"}}]
}
```

### Provider selection logic

In `run_ask`, after resolving the model name:

```
provider_name = first segment of model name before "/"
              (e.g. "anthropic" from "anthropic/claude-sonnet-4-5")

if config.api_key(provider_name) is Some:
    use the named provider directly (AnthropicProvider for "anthropic")
else if config.api_key("openrouter") is Some:
    use OpenRouterProvider (pass full model string through)
else:
    error: no API key configured
```

This means:
- Existing Anthropic users are unaffected — no config changes needed.
- OpenRouter users set `[providers.openrouter]` and use any model string.
- A user can have both configured; the direct provider takes precedence.

### Config — no schema changes needed

`AskConfig.providers` is already `HashMap<String, ProviderConfig>`. Adding OpenRouter
support requires no struct changes — it is just a new key:

```toml
[providers.openrouter]
api_key = "sk-or-..."
```

---

## Tasks

- [ ] **`clank-ask/src/provider/openrouter.rs`** — implement `OpenRouterProvider`
  - [ ] Define `ApiRequest` / `ApiResponse` serde structs (OpenAI format)
  - [ ] `OpenRouterProvider::new(api_key, Arc<dyn HttpClient>)` with overridable `base_url`
  - [ ] Implement `ModelProvider::complete()`:
    - POST to `/api/v1/chat/completions`
    - Headers: `Authorization: Bearer`, `Content-Type: application/json`,
      `HTTP-Referer: https://clank.sh`, `X-OpenRouter-Title: clank.sh`
    - Pass full model string (no prefix stripping)
    - Parse `choices[0].message.content` from response
    - Map HTTP errors to `ProviderError` variants (401/402 → `NotConfigured`,
      429 → `Timeout`, 502/503 → `RemoteCallFailed`)
  - [ ] Unit tests with `MockHttpClient`:
    - `test_openrouter_mock_success` — parses response correctly
    - `test_openrouter_builds_correct_request` — full model string passed, correct headers
    - `test_openrouter_mock_timeout` — 429 → `ProviderError::Timeout`, exit 3
    - `test_openrouter_mock_http_error` — 502 → `ProviderError::RemoteCallFailed`, exit 4
    - `test_openrouter_mock_unauthorized` — 401 → `ProviderError::NotConfigured`, exit 1

- [ ] **`clank-ask/src/provider/mod.rs`** — export `OpenRouterProvider`

- [ ] **`clank-ask/src/ask_process.rs`** — update provider selection
  - [ ] Extract provider selection into a `select_provider` function
  - [ ] Logic: direct provider if key present, else openrouter fallback, else error
  - [ ] Unit tests:
    - `test_provider_selection_anthropic_direct` — anthropic key → AnthropicProvider
    - `test_provider_selection_openrouter_fallback` — no anthropic key, openrouter key →
      OpenRouterProvider
    - `test_provider_selection_no_key_exits_1` — no keys → exit 1 informative message
    - `test_provider_selection_openrouter_with_anthropic_model` — `anthropic/claude-...`
      model routed through OpenRouter when only openrouter key present

- [ ] **`TUTORIAL.md`** — add OpenRouter configuration section after section 5

- [ ] **`OVERVIEW.md`** — mention OpenRouter support in the "What has been built" section

- [ ] **Final quality gate**
  - [ ] `cargo test` — all tests pass
  - [ ] `cargo clippy --all-targets -- -D warnings` — clean
  - [ ] `cargo fmt --check` — clean

---

## Acceptance Tests

| # | Test | Location | Assertion |
|---|---|---|---|
| 1 | `test_openrouter_mock_success` | `provider/openrouter.rs` | Response text extracted from `choices[0].message.content` |
| 2 | `test_openrouter_builds_correct_request` | `provider/openrouter.rs` | Full model string in request; `Authorization: Bearer` header present |
| 3 | `test_openrouter_mock_timeout` | `provider/openrouter.rs` | 429 → exit code 3 |
| 4 | `test_openrouter_mock_http_error` | `provider/openrouter.rs` | 502 → exit code 4 |
| 5 | `test_openrouter_mock_unauthorized` | `provider/openrouter.rs` | 401 → exit code 1 |
| 6 | `test_provider_selection_anthropic_direct` | `ask_process.rs` | Anthropic key → direct |
| 7 | `test_provider_selection_openrouter_fallback` | `ask_process.rs` | OpenRouter fallback |
| 8 | `test_provider_selection_no_key_exits_1` | `ask_process.rs` | No keys → exit 1 + message |
| 9 | `test_provider_selection_openrouter_with_anthropic_model` | `ask_process.rs` | Full model string passed through |

All existing `clank-ask` tests must continue to pass unchanged.
