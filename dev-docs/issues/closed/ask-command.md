---
title: "Implement the ask command"
date: 2026-03-08
author: agent
---

## Problem

`ask` is the primary human-AI interface in clank.sh. Per the README it lives at
`/usr/bin/ask`, receives the current sliding-window transcript as context plus
any content piped to its stdin, calls the configured model provider, and streams
the response to stdout.

The command does not exist yet. Typing `ask "what is this?"` currently produces
a command-not-found error. The provider layer introduced in #5 (`clank-provider`
with Ollama and OpenRouter backends, `~/.config/ask/ask.toml` configuration) is
in place, so the infrastructure needed to call a model is already available.
What is missing is the `ask` command itself.

## Capability Gap

Without `ask`:

- There is no way to invoke the model from the shell prompt.
- The transcript-as-context story is not exercisable end-to-end.
- Piped input (`cat error.log | ask "summarize this"`) does not work.
- The `--fresh` / `--no-transcript` flags for scripting use-cases are absent.

## Scope

This issue covers an initial, functional `ask` implementation sufficient for
interactive and scripted use:

- `ask` installed as a subprocess-scoped command (not a builtin) reachable on
  `$PATH`.
- Reads the current transcript as context (default behaviour).
- Accepts a prompt as a positional argument and/or reads a prompt from stdin
  when stdin is not a TTY.
- Calls the configured provider via `clank-provider` and prints the response to
  stdout.
- Supports `--fresh` / `--no-transcript` to suppress transcript context for
  scripting use.
- Response is recorded in the transcript as an `AiResponse` entry (new entry
  kind) so subsequent `context show` and future `context summarize` calls see
  the full exchange.

This does not cover: tool calling, `--json` structured output, `ask repl`,
`ask --model` override, streaming, or the system prompt at
`/proc/clank/system-prompt`.
