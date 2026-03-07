---
title: "Local Model Providers — Research"
date: 2026-03-06
author: agent
---

# Local Model Providers — Research

## Motivation

clank.sh currently requires a network call and an API key for every `ask` invocation.
Users who want privacy, offline operation, or zero-cost iteration cannot use the shell as
an AI tool without a cloud account. Supporting local inference backends would make clank
useful in these contexts.

This document surveys the viable local inference backends, their wire APIs, and the
implementation approach that fits best with clank's existing provider architecture.

---

## Local inference landscape

Three backends account for the overwhelming majority of local LLM usage:

### 1. Ollama

**What it is:** A desktop-class inference daemon that downloads and runs quantised models
(GGUF, Safetensors) with a simple CLI and a REST API. Default port: `11434`.

**API surface:**

Ollama exposes two relevant HTTP endpoints:

#### `/api/generate` (raw completion)

```
POST http://localhost:11434/api/generate
Content-Type: application/json

{
  "model": "llama3.2",
  "prompt": "Why is the sky blue?",
  "system": "You are a helpful assistant.",
  "stream": false
}
```

Response (non-streaming):
```json
{
  "model": "llama3.2",
  "response": "The sky appears blue because...",
  "done": true
}
```

#### `/api/chat` (multi-turn, OpenAI-style messages)

```
POST http://localhost:11434/api/chat
Content-Type: application/json

{
  "model": "llama3.2",
  "messages": [
    { "role": "system", "content": "You are a helpful assistant." },
    { "role": "user",   "content": "Why is the sky blue?" }
  ],
  "stream": false
}
```

Response:
```json
{
  "model": "llama3.2",
  "message": { "role": "assistant", "content": "The sky appears blue because..." },
  "done": true
}
```

**Authentication:** None required. No API key.

**Model naming:** Plain names like `"llama3.2"`, `"mistral"`, `"phi4"`, `"qwen2.5"`.
No provider prefix. The model must already be pulled (`ollama pull llama3.2`).

**OpenAI compatibility layer:** Ollama also exposes `/v1/chat/completions` at the same
port with OpenAI-compatible request/response format. This would let clank reuse
`OpenRouterProvider`'s wire format by pointing it at `http://localhost:11434/v1`.
However, streaming is not required by clank, and the native `/api/chat` endpoint is
simpler and more reliable for non-streaming use.

**Error cases:**
- Model not found: HTTP `404`, body `{"error":"model 'xyz' not found, try pulling it first"}`
- Server not running: connection refused

**Availability check:** `GET /api/tags` — lists installed models. Returns HTTP `200` with
a JSON object if Ollama is running, connection refused otherwise.

---

### 2. llama.cpp server (`llama-server`)

**What it is:** The canonical C++ inference engine. The `llama-server` binary exposes an
OpenAI-compatible HTTP API. Default port: `8080`.

**API surface:**

Exposes `/v1/chat/completions` with the standard OpenAI request/response format:

```
POST http://localhost:8080/v1/chat/completions
Content-Type: application/json
Authorization: Bearer no-key-required

{
  "model": "any-string-ignored",
  "messages": [
    { "role": "system", "content": "..." },
    { "role": "user",   "content": "..." }
  ],
  "stream": false
}
```

The `model` field is ignored — llama-server serves exactly one model, specified at
startup via `--model <path.gguf>`. The `Authorization` header is accepted but not
validated unless `--api-key` is passed at startup.

**Authentication:** Optional. If `--api-key <key>` is set, the `Authorization: Bearer`
header must match. If not set, any value (including empty) is accepted.

**Error cases:**
- Startup failure: process exits; connection refused
- Invalid request: HTTP `400`

**Relationship to OpenRouter:** The wire format is identical to `OpenRouterProvider`.
llama-server is a drop-in replacement for the HTTP layer if the base URL is configurable.

---

### 3. LM Studio

**What it is:** A GUI application for macOS, Windows, and Linux that runs local models.
It exposes an OpenAI-compatible server on port `1234` when enabled.

**API surface:** Identical to llama.cpp server (`/v1/chat/completions`), OpenAI format.

**Authentication:** None (local only).

**Relevance:** Same wire format as llama.cpp. No additional implementation needed if the
base URL is configurable.

---

## Comparison summary

| Backend | Default port | Wire format | Auth | Model naming |
|---|---|---|---|---|
| Ollama | `11434` | Native (`/api/chat`) or OpenAI (`/v1`) | None | Plain name |
| llama.cpp | `8080` | OpenAI (`/v1/chat/completions`) | Optional bearer | Ignored |
| LM Studio | `1234` | OpenAI (`/v1/chat/completions`) | None | Arbitrary |

---

## Key design decision: one provider or two?

### Option A: Single `LocalProvider` with runtime backend detection

Detect at request time which backend is running (try Ollama first, then OpenAI-compat).
**Rejected:** Too clever. Detection is fragile, slow, and surprising to users. A user
running both Ollama and llama-server on different ports needs explicit control.

### Option B: Two providers — `OllamaProvider` + `OpenAiCompatProvider`

`OllamaProvider`: uses Ollama's native `/api/chat` endpoint. No auth, no prefix stripping.

`OpenAiCompatProvider`: generic OpenAI-compatible endpoint at a user-configured base URL.
Covers llama.cpp, LM Studio, vLLM, LocalAI, and any future OpenAI-compatible server.

**Recommended.** Each backend has a distinct config entry; the user chooses explicitly.

---

## Config schema changes

### New provider keys in `ask.toml`

```toml
# Ollama (local)
[providers.ollama]
base_url = "http://localhost:11434"   # optional; default shown

# Generic OpenAI-compatible endpoint (llama.cpp, LM Studio, vLLM, etc.)
[providers.openai-compat]
base_url = "http://localhost:8080"    # required
api_key  = ""                          # optional; sent as Bearer if non-empty
```

### No `api_key` required for Ollama

`ProviderConfig` already has `api_key: Option<String>`. The new providers simply
treat a missing/empty `api_key` as "no authentication required" — already the default.

A new field `base_url: Option<String>` must be added to `ProviderConfig`:

```rust
pub struct ProviderConfig {
    pub api_key: Option<String>,
    pub base_url: Option<String>,   // new
}
```

---

## Model string conventions

### Ollama

Users write `ollama/llama3.2` in clank. The `"ollama/"` prefix is stripped before
sending to the Ollama API (which expects just `"llama3.2"`).

This matches the existing pattern: `AnthropicProvider` already strips `"anthropic/"`.

### OpenAI-compatible

Users write `openai-compat/phi4` (or any arbitrary string after the prefix). The prefix
is stripped and the remainder is sent as the `model` field. For llama-server this is
ignored, but it allows future servers that do use the model field.

---

## `select_provider` changes

Current `select_provider` in `ask_process.rs:222`:

```rust
match provider_prefix {
    "anthropic" => ...AnthropicProvider::new(key, http),
    _ => ...OpenRouterProvider::new(key, http),
}
```

With local providers:

```rust
match provider_prefix {
    "anthropic"     => Box::new(AnthropicProvider::new(key, http)),
    "ollama"        => Box::new(OllamaProvider::new(base_url, http)),
    "openai-compat" => Box::new(OpenAiCompatProvider::new(base_url, api_key, http)),
    _               => Box::new(OpenRouterProvider::new(key, http)),
}
```

`base_url` comes from `config.providers[prefix].base_url` with the appropriate default.

---

## `OllamaProvider` wire details

### Request

```
POST {base_url}/api/chat
Content-Type: application/json

{
  "model": "{model_without_prefix}",
  "messages": [
    { "role": "system", "content": "{system_prompt}" },
    { "role": "user",   "content": "{user_message}" }
  ],
  "stream": false
}
```

### Response parsing

```json
{
  "message": { "role": "assistant", "content": "..." },
  "done": true
}
```

Extract `response.message.content`.

### Error mapping

| Condition | `ProviderError` |
|---|---|
| Connection refused | `RemoteCallFailed("Ollama is not running at {base_url}. Start it with: ollama serve")` |
| HTTP 404 | `RemoteCallFailed("Model '{model}' not found. Pull it with: ollama pull {model}")` |
| HTTP 5xx | `RemoteCallFailed("Ollama returned {status}: {body}")` |
| Timeout | `Timeout` |

---

## `OpenAiCompatProvider` wire details

The request/response format is identical to `OpenRouterProvider`, with two differences:
1. The base URL is configurable (`http://localhost:8080` default).
2. The `Authorization: Bearer` header is omitted if `api_key` is empty.

This provider can be implemented as a thin wrapper around the existing
`OpenRouterProvider` logic with a configurable base URL, or as a new struct that shares
the request/response serialisation code.

---

## `model add` interaction

`model add ollama` needs no API key but should confirm the server is reachable:

```sh
model add ollama                       # uses http://localhost:11434
model add ollama --url http://host:11434
model add openai-compat --url http://localhost:8080
model add openai-compat --url http://localhost:8080 --key sk-optional
```

`model add` is currently a stub (`exit 1`). Implementing local providers requires it to
be able to write `base_url` alongside `api_key` into `ask.toml`.

---

## Streaming

clank currently does not use streaming — `ask` waits for the full response. Both Ollama
and llama.cpp support non-streaming mode (`"stream": false`). No streaming implementation
is needed for this feature.

---

## WASM / Golem portability

Both providers communicate via HTTP. HTTP calls already go through the `HttpClient` trait
(`clank-http`). No changes to the WASM portability boundary are needed — the same
abstraction holds.

---

## Out of scope

- Streaming output (Phase 5+).
- Model download / management via clank (`ollama pull` is out of scope — users manage
  their local models with their local tool of choice).
- LM Studio's model management API.
- Embeddings endpoints.
- Function/tool calling support.
