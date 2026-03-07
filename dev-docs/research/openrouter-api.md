---
title: "OpenRouter API — Wire Format and Integration Research"
date: 2026-03-06
author: agent
---

# OpenRouter API — Wire Format and Integration Research

## Motivation

clank.sh currently supports only the Anthropic provider directly. OpenRouter is a unified
routing layer that proxies requests to hundreds of models (Anthropic, OpenAI, Google, Mistral,
Meta, etc.) through a single API. Supporting OpenRouter would give users access to every
model on that network without clank needing to implement each provider individually.

## API overview

OpenRouter exposes an OpenAI-compatible chat completions API.

### Endpoint

```
POST https://openrouter.ai/api/v1/chat/completions
```

### Authentication

```
Authorization: Bearer <OPENROUTER_API_KEY>
```

Two optional headers identify the calling application (used for OpenRouter's app rankings;
not required for functionality):

```
HTTP-Referer: https://clank.sh
X-OpenRouter-Title: clank.sh
```

### Request body

```json
{
  "model": "anthropic/claude-sonnet-4-5",
  "max_tokens": 4096,
  "system": "...",
  "messages": [
    {"role": "user", "content": "..."}
  ]
}
```

The `model` field uses `provider/model-name` notation — the same convention clank already
uses internally (`config.resolve_model()` strips the prefix when calling Anthropic directly).
With OpenRouter, the full `provider/model-name` string is passed through unchanged.

`system` is supported as a top-level field (same as Anthropic). OpenRouter normalises it
across providers that use different system prompt conventions.

### Response body

```json
{
  "id": "gen-...",
  "choices": [
    {
      "message": {
        "role": "assistant",
        "content": "..."
      },
      "finish_reason": "stop"
    }
  ],
  "usage": {
    "prompt_tokens": 42,
    "completion_tokens": 128,
    "total_tokens": 170
  }
}
```

This is the OpenAI chat completions format. Note the key difference from Anthropic's format:

| | Anthropic | OpenRouter |
|---|---|---|
| Response text path | `content[0].text` | `choices[0].message.content` |
| Auth header | `x-api-key` | `Authorization: Bearer` |
| Version header | `anthropic-version: 2023-06-01` | (none) |
| System prompt | top-level `system` field | top-level `system` field ✓ |
| Model name | stripped prefix (`claude-sonnet-4-5`) | full string (`anthropic/claude-sonnet-4-5`) |

### Error responses

OpenRouter returns standard HTTP error codes with JSON bodies:

| Code | Meaning | clank exit code |
|---|---|---|
| 401 | Invalid or missing API key | 1 (not configured) |
| 402 | Insufficient credits | 1 |
| 429 | Rate limited | 3 (timeout-like) |
| 502 | Provider upstream failure | 4 |
| 503 | Service unavailable | 4 |

## Comparison with current Anthropic provider

The `AnthropicProvider` in `clank-ask/src/provider/anthropic.rs` implements `ModelProvider`
with a single `complete()` method. An `OpenRouterProvider` would implement the same trait
with a different request/response serialisation.

The two providers differ only in:
1. The URL (`api.anthropic.com` vs `openrouter.ai`)
2. The auth header name and format
3. The request JSON schema (Anthropic's Messages API vs OpenAI chat completions)
4. The response JSON parsing path
5. Model name handling (prefix stripping vs pass-through)

Everything else in `run_ask` — transcript embedding, system prompt assembly, flag parsing,
exit code mapping, `--json` contract — is unchanged.

## Provider selection logic

Currently `run_ask` hardcodes `AnthropicProvider`. The selection logic needs to be based on
the resolved model name or an explicit provider field in config.

**Option A: Detect from model name prefix**
If the model contains a `/`, the part before the `/` is the provider. Map `"anthropic"` →
`AnthropicProvider`, `"openrouter"` → `OpenRouterProvider`, everything else → default to
`OpenRouterProvider` (since OpenRouter accepts any `provider/model` string).

**Option B: Explicit `provider` field in config**
Add a top-level `provider = "openrouter"` field to `ask.toml`. More explicit but more
verbose.

**Recommendation: Option A.** The model name already encodes the provider. Add a special
`"openrouter"` provider key in config for the API key, and route to `OpenRouterProvider`
when the model string does not match a directly-supported provider. This means:
- `default_model = "anthropic/claude-sonnet-4-5"` + `[providers.anthropic]` → uses
  `AnthropicProvider` directly
- `default_model = "openai/gpt-4o"` + `[providers.openrouter]` → uses `OpenRouterProvider`
- `default_model = "anthropic/claude-sonnet-4-5"` + `[providers.openrouter]` → uses
  `OpenRouterProvider` (routing through OpenRouter to reach Anthropic)

## Config schema extension

```toml
default_model = "anthropic/claude-sonnet-4-5"

# Direct Anthropic access
[providers.anthropic]
api_key = "sk-ant-..."

# OR: OpenRouter access (reaches any model)
[providers.openrouter]
api_key = "sk-or-..."
```

If both are configured, the resolved provider name (from the model prefix) takes precedence.
`"openrouter"` is the fallback if no matching direct provider is found.

## Implementation scope

- New file: `crates/clank-ask/src/provider/openrouter.rs`
- Updated: `crates/clank-ask/src/ask_process.rs` — provider selection logic
- Updated: `crates/clank-ask/src/config.rs` — `AskConfig` struct (no schema change needed;
  `providers` is already a `HashMap<String, ProviderConfig>`, so `openrouter` is just
  another key)
- Updated: `TUTORIAL.md` and `OVERVIEW.md` — document OpenRouter as a supported option

No changes needed to `clank-shell`, `clank-manifest`, or the binary crate.

## References

- OpenRouter API docs: https://openrouter.ai/docs/api-reference/overview
- OpenRouter model list: https://openrouter.ai/models
- OpenRouter quick start: https://openrouter.ai/docs/quick-start
