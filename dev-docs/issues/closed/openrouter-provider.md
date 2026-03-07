---
title: "Add OpenRouter as a supported model provider"
date: 2026-03-06
author: agent
---

# Add OpenRouter as a supported model provider

## Problem

clank.sh currently supports only the Anthropic API directly. Users who have an OpenRouter
account cannot use it, and users who want to access non-Anthropic models (OpenAI, Google,
Mistral, Meta, etc.) have no path to do so.

## Capability Gap

- `[providers.openrouter]` in `ask.toml` has no effect — the key is ignored.
- Any model whose prefix is not `"anthropic"` fails with "no API key configured".
- Users who route Anthropic calls through OpenRouter (for unified billing, fallbacks, or
  latency optimisation) cannot do so.

## Deliverables

- `OpenRouterProvider` implementing `ModelProvider` via the OpenAI-compatible chat
  completions API at `https://openrouter.ai/api/v1/chat/completions`.
- Provider selection in `run_ask`: detect provider from model name prefix; fall back to
  OpenRouter for any model not matching a directly-supported provider.
- `[providers.openrouter] api_key = "..."` in `ask.toml` is recognised and used.
- Unit tests with `MockHttpClient` covering success, timeout, HTTP error, and request format.
- `TUTORIAL.md` updated with OpenRouter configuration instructions.
