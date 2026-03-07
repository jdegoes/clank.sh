---
title: ask — Anthropic Messages API and Request Construction
date: 2026-03-07
author: agent
---

## Purpose

Determine the exact Anthropic Messages API request/response format, how to
construct the request from the transcript and user prompt, and how `ask` fits
into the existing `ClankShell` architecture.

---

## Anthropic Messages API (MVP target)

### Endpoint

```
POST https://api.anthropic.com/v1/messages
```

### Required headers

```
x-api-key: <key>
anthropic-version: 2023-06-01
Content-Type: application/json
```

### Request body

```json
{
  "model": "claude-sonnet-4-5",
  "max_tokens": 4096,
  "system": "<system prompt>",
  "messages": [
    { "role": "user", "content": "<transcript + user prompt>" }
  ]
}
```

For MVP: single user message containing the formatted transcript followed by
the user's prompt. No multi-turn message array — the transcript already encodes
the full conversation history in rendered form.

### Response body

```json
{
  "id": "msg_...",
  "type": "message",
  "role": "assistant",
  "content": [{ "type": "text", "text": "..." }],
  "model": "claude-sonnet-4-5-20251111",
  "stop_reason": "end_turn",
  "usage": { "input_tokens": 42, "output_tokens": 17 }
}
```

The response text is `content[0].text` for MVP (only `type: "text"` blocks
need to be handled).

---

## Model Name Resolution

The config stores e.g. `"anthropic/claude-sonnet-4-5"`. The Anthropic API
expects just `"claude-sonnet-4-5-20251111"` (with date suffix). For MVP,
strip the provider prefix and pass the model name as-is — Anthropic accepts
short names like `"claude-sonnet-4-5"` without the date suffix in practice.

---

## Context Construction

The user message sent to the model is:

```
<transcript rendered via format_for_model()>

<piped stdin if any>

<user prompt>
```

With `--fresh`, only the user prompt is sent (no transcript, no stdin).

---

## Where `ask` Lives

`ask` is intercepted in `run_repl()` in `clank/src/lib.rs`, same as `context`
and `model`. It needs access to:

1. The transcript (`ClankShell::transcript`)
2. The `HttpClient` (`Arc<dyn HttpClient>`)
3. The config (`clank_config::load_config()`)

Since `run_repl()` already owns `ClankShell`, passing `Arc<dyn HttpClient>`
into `run_repl()` is the cleanest approach. `ClankShell` gains an `ask` method
that takes the client, the prompt, and flags.

---

## System Prompt (MVP)

A minimal system prompt describing the environment:

```
You are an AI agent operating a bash-compatible shell called clank.sh.
The transcript above shows the session history. Respond helpfully and concisely.
```

The full system prompt described in the README (filesystem map, tool descriptions,
`/proc/clank/system-prompt`) is deferred — it is a prompt engineering concern
for a later task.

---

## Error Handling

- Config not found / no default model → print error to stderr, exit
- API key not configured → print error to stderr, exit
- HTTP error (non-2xx) → print error message + status to stderr
- Successful response → print `content[0].text` to stdout, append to transcript

---

## Conclusions

1. Anthropic Messages API: POST to `/v1/messages` with `x-api-key`,
   `anthropic-version: 2023-06-01`, JSON body with `model`, `max_tokens`,
   `system`, `messages`.
2. Response text: `content[0].text`.
3. Context: `transcript.format_for_model()` + optional stdin + user prompt.
4. `ask` is intercepted in `run_repl()` — not a Brush builtin.
5. `Arc<dyn HttpClient>` passed into `run_repl()` and `ClankShell::ask()`.
6. System prompt: minimal for MVP.
7. Model name: strip provider prefix, pass to API as-is.
