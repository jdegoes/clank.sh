---
title: "Local model providers (Ollama, llama.cpp, LM Studio)"
date: 2026-03-06
author: agent
---

# Local Model Providers

## Problem

Every `ask` invocation requires a network round-trip and a cloud API key. Users who want
privacy (no data leaving their machine), offline operation, zero inference cost, or faster
iteration with smaller models cannot use clank.sh as an AI tool without a cloud account.

## Capability Gap

- There is no way to configure clank to call a locally-running inference server.
- `ask --model ollama/llama3.2 "..."` fails — no `ollama` provider exists.
- `model add ollama` and `model add openai-compat` do not exist.
- `ProviderConfig` has no `base_url` field — local servers cannot be addressed.

## Deliverables

Two new `ModelProvider` implementations and the config plumbing to wire them:

### `OllamaProvider`

- Uses Ollama's native `/api/chat` endpoint (not the OpenAI compatibility layer).
- No API key required.
- Model string convention: `ollama/<name>` in clank; `<name>` sent to Ollama.
- Informative errors when Ollama is not running or the model has not been pulled.
- `model add ollama [--url <base_url>]` writes the provider entry to `ask.toml`.

### `OpenAiCompatProvider`

- Generic provider for any server exposing `/v1/chat/completions` in OpenAI format.
- Covers llama.cpp (`llama-server`), LM Studio, vLLM, LocalAI, and future servers.
- Optional API key (sent as `Authorization: Bearer` only if non-empty).
- Required `base_url` config field (no sensible default exists across tools).
- Model string convention: `openai-compat/<name>` in clank; `<name>` sent as the
  `model` field (ignored by single-model servers like llama-server, used by others).
- `model add openai-compat --url <base_url> [--key <api_key>]` writes the entry.

### Config schema extension

`ProviderConfig` gains a `base_url: Option<String>` field. Serialised in `ask.toml`:

```toml
[providers.ollama]
base_url = "http://localhost:11434"   # optional; shown is the default

[providers.openai-compat]
base_url = "http://localhost:8080"    # required
api_key  = ""                          # optional
```

### `model add` partial implementation

The stub implementation of `model add` must be extended to support at minimum:

```sh
model add ollama [--url <base_url>]
model add openai-compat --url <base_url> [--key <api_key>]
```

Writing config for key-based providers (`model add anthropic --key <KEY>`) may be
implemented at the same time or deferred — the issue does not require it, but touching
`model add` for local providers makes it natural to do both.

## Open Questions Requiring Design

- **`model add` for all existing providers.** The stub currently covers `add`, `remove`,
  `default`, and `info` uniformly. Partially implementing `add` for local providers only
  (and leaving the rest stubbed) is acceptable if the design is explicit about it.
- **Default `base_url` for `openai-compat`.** There is no single canonical default
  (llama-server uses `8080`, LM Studio uses `1234`, vLLM uses `8000`). The field should
  be required, with no default applied silently.

## Out of Scope

- Streaming responses (Phase 5+).
- Local model download or management via clank (`ollama pull`, model file management).
- Embeddings, function calling, or vision endpoints.
- Auto-detection of running local servers.
