---
title: "Ollama and OpenRouter API research"
date: 2026-03-07
author: agent
---

## Purpose

This document records findings on the Ollama and OpenRouter HTTP APIs as they relate to implementing `context summarize` and, eventually, the broader `ask` command. It covers: endpoint structure, request/response schemas, authentication, non-streaming completion, and the key differences between the two providers.

---

## Ollama

**Source:** https://github.com/ollama/ollama/blob/main/docs/api.md

### Overview

Ollama runs as a local HTTP server (default `http://localhost:11434`). No authentication — all requests are unauthenticated by default. Models are referenced by `name:tag` (e.g. `llama3.2`, `llama3.2:8b`).

### Chat completions endpoint

```
POST /api/chat
```

This is the correct endpoint for `context summarize`: it accepts a messages array (system + user roles), mirrors the OpenAI chat interface closely, and supports both streaming and non-streaming modes.

**Minimal non-streaming request:**

```json
{
  "model": "llama3.2",
  "messages": [
    { "role": "system", "content": "You are a helpful assistant." },
    { "role": "user",   "content": "Summarize this transcript: ..." }
  ],
  "stream": false
}
```

**Response (non-streaming):**

```json
{
  "model": "llama3.2",
  "created_at": "2023-12-12T14:13:43.416799Z",
  "message": {
    "role": "assistant",
    "content": "The session covered..."
  },
  "done": true,
  "total_duration": 5191566416,
  "load_duration": 2154458,
  "prompt_eval_count": 26,
  "prompt_eval_duration": 383809000,
  "eval_count": 298,
  "eval_duration": 4799921000
}
```

The response text is at `.message.content`.

### Key parameters

| Parameter | Type | Notes |
|---|---|---|
| `model` | string | Required. `name:tag` format. |
| `messages` | array | `{ role, content }` objects. roles: `system`, `user`, `assistant`, `tool`. |
| `stream` | bool | Default `true`. Set `false` for single-response mode. |
| `options` | object | Temperature, top_k, top_p, seed, etc. All optional. |
| `keep_alive` | string/number | How long the model stays loaded. Default `"5m"`. |

### Error handling

Ollama returns HTTP 200 for all responses including errors in streaming mode. For non-streaming, a server error returns a non-2xx status with a JSON body `{ "error": "<message>" }`.

Connection refused (server not running) surfaces as a transport error from the HTTP client.

### Generate endpoint (alternative)

```
POST /api/generate
```

Single-turn, no conversation history. Simpler but less suitable for chat-style summarization prompts. Not preferred for this use case.

---

## OpenRouter

**Source:** https://openrouter.ai/docs/api/reference/overview.mdx

### Overview

OpenRouter is a hosted proxy that normalizes access to hundreds of models under a single OpenAI-compatible API. Endpoint: `https://openrouter.ai/api/v1`. Authentication via Bearer token in the `Authorization` header.

### Chat completions endpoint

```
POST /api/v1/chat/completions
Authorization: Bearer <OPENROUTER_API_KEY>
Content-Type: application/json
```

**Request schema** (subset relevant to `context summarize`):

```json
{
  "model": "anthropic/claude-3-5-haiku",
  "messages": [
    { "role": "system", "content": "You are a helpful assistant." },
    { "role": "user",   "content": "Summarize this transcript: ..." }
  ],
  "stream": false,
  "max_tokens": 1024
}
```

**Response schema** (OpenAI-compatible):

```json
{
  "id": "gen-xxxxxxxxxxxxxx",
  "choices": [
    {
      "finish_reason": "stop",
      "message": {
        "role": "assistant",
        "content": "The session covered..."
      }
    }
  ],
  "usage": {
    "prompt_tokens": 200,
    "completion_tokens": 150,
    "total_tokens": 350
  },
  "model": "anthropic/claude-3-5-haiku"
}
```

The response text is at `.choices[0].message.content`.

### Key parameters

| Parameter | Type | Notes |
|---|---|---|
| `model` | string | Required. `provider/name` format (e.g. `anthropic/claude-3-5-haiku`, `openai/gpt-4o-mini`). |
| `messages` | array | `{ role, content }`. Same roles as OpenAI. |
| `stream` | bool | Default `false`. |
| `max_tokens` | integer | Range `[1, context_length)`. |
| `temperature` | float | Range `[0, 2]`. |

### Authentication

`Authorization: Bearer <key>` header on every request. API key created at https://openrouter.ai/keys. For `context summarize`, the key is read from `~/.config/ask/ask.toml`.

### Error handling

Standard HTTP status codes: 400 (bad request), 401 (unauthorized), 402 (insufficient credits), 429 (rate limited), 500/503 (provider error). Error body:

```json
{
  "error": {
    "code": 401,
    "message": "No auth credentials found",
    "metadata": {}
  }
}
```

### Optional headers

`HTTP-Referer` and `X-OpenRouter-Title` can identify the application in OpenRouter's analytics. For clank.sh, use `HTTP-Referer: https://clank.sh` and `X-OpenRouter-Title: clank.sh`.

---

## Comparison: Ollama vs OpenRouter for `context summarize`

| Aspect | Ollama | OpenRouter |
|---|---|---|
| Endpoint | `http://localhost:11434/api/chat` | `https://openrouter.ai/api/v1/chat/completions` |
| Auth | None | Bearer token |
| Response path | `.message.content` | `.choices[0].message.content` |
| Model format | `name:tag` | `provider/name` |
| Streaming default | `true` | `false` |
| Error body | `{ "error": "..." }` | `{ "error": { "code", "message" } }` |

The response schemas differ — Ollama returns `{ message: { content } }` while OpenRouter returns `{ choices: [{ message: { content } }] }`. Each provider implementation must parse its own schema and return a common string.

---

## Configuration design

The README specifies `~/.config/ask/ask.toml` for default model and provider keys. A minimal TOML schema for the initial implementation:

```toml
[provider]
name = "ollama"           # or "openrouter"
base_url = "http://localhost:11434"  # required for ollama; not needed for openrouter
model = "llama3.2"

[provider.openrouter]
api_key = "sk-or-..."
```

For `context summarize`, only the `[provider]` table is needed. The API key is provider-specific and must be in `[provider.openrouter]` when using OpenRouter. Keys must not be logged or exposed in the transcript.

A simpler alternative that avoids a nested table and covers the initial use case:

```toml
provider = "ollama"
model = "llama3.2"
base_url = "http://localhost:11434"   # used only for ollama
openrouter_api_key = "sk-or-..."      # used only for openrouter
```

The flat schema is easier to implement and sufficient for the initial scope. It can be extended later without breaking existing configs.

---

## JSON parsing crate choice

The response bodies are JSON. The workspace uses `reqwest` with the `json` feature (already in `clank-http`), which pulls in `serde` and `serde_json` as transitive dependencies. `serde_json` is available in the workspace already by virtue of this. `serde` with `derive` will be needed to define request/response structs. Both are zero-cost in the WASM sense — they are pure Rust.

`toml` is needed to parse `ask.toml`. The `toml` crate (`toml = "0.8"`) with `serde` feature is the standard choice. It is not currently in the workspace.

---

## HTTP POST requirement

The current `clank-http` `HttpClient` trait only has `get()`. Provider API calls require `POST` with a JSON body and custom headers (at minimum `Content-Type: application/json`, and for OpenRouter, `Authorization: Bearer ...`). The trait needs a `post()` method or must be extended. Given that `clank-provider` will depend on `clank-http`, the cleanest approach is to add `post()` to `HttpClient` with a body and headers parameter before implementing the providers.

`reqwest` already supports POST — the `NativeHttpClient` implementation just needs to expose it.
