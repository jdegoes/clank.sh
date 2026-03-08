---
title: "context summarize: implement LLM-backed transcript summary"
date: 2026-03-07
author: agent
---

## Problem

`context summarize` is a documented shell-internal builtin that prints a natural-language summary of the current transcript to stdout. It is specified in the README and is the foundation for automatic transcript compaction — the mechanism by which the sliding window replaces its leading edge with a summary block when the token budget is approached.

The command is currently unimplemented: it exits with code 2 (unknown subcommand). This makes the transcript management story incomplete: `context show` and `context clear` work, but `context summarize` — the essential primitive for bounded, useful context windows — does not.

## Capability Gap

The shell has no mechanism to call any LLM API. There is no provider abstraction, no configuration for model endpoints or API keys, and no concrete implementation for any model provider. `clank-http` provides a generic `HttpClient` trait but nothing that understands a model API request or response format.

Until this gap is filled:

- `context summarize` cannot be implemented.
- Automatic transcript compaction (sliding window summarization) cannot be implemented.
- The `ask` command — which will also need provider infrastructure — has no foundation to build on.

## Scope

This issue covers the minimum necessary to make `context summarize` work end-to-end:

1. A `clank-provider` crate that defines the provider trait and implements Ollama and OpenRouter as the first two concrete providers.
2. Provider configuration: which provider to use, endpoint URL, and API key, read from `~/.config/ask/ask.toml`.
3. `context summarize` wired up to call the configured provider with the transcript content and print the resulting summary to stdout.

This does not cover: the full `ask` command, `model list/add/remove/default`, automatic compaction, tool calling, streaming output, or any provider beyond Ollama and OpenRouter.
