---
title: ask command not implemented — the AI has no voice
date: 2026-03-07
author: agent
---

## Summary

Without `ask`, clank.sh is a shell with memory but no AI. The transcript records
everything, the model is configured, the HTTP client works — but there is no way
to send any of it to a model and get a response. `ask` is the command that makes
clank.sh what it is.

From the README:

> "`ask` is a regular process. No bimodal shell, no syntax disambiguation, no
> mode detection."

## Problem

There is no `ask` command. An operator (human or AI) cannot invoke the AI model.
The transcript is built up but never sent anywhere. The `model` configuration is
stored but never read by anything that calls a model.

## What `ask` Must Do (MVP scope)

1. Read the current transcript from `ClankShell`
2. Optionally read supplementary context from stdin (piped input)
3. Construct a request to the configured model provider (Anthropic for MVP)
4. Send it via `clank-http`'s `HttpClient`
5. Print the response to stdout
6. Append the AI response to the transcript as a `TranscriptEntry::AiResponse`

## What `ask` Is (Architecture)

`ask` is `shell-internal` scoped — it is intercepted in the REPL directly, like
`context` and `model`. It reads from `ClankShell`'s transcript and writes back
to it. It does not spawn an OS process.

## MVP Flags

| Flag | Behaviour |
|---|---|
| (none) | Use current transcript as context + user prompt |
| `--fresh` | Send only the user prompt, no transcript context |
| `--model <m>` | Override the default model for this invocation |

`--json`, `ask repl`, `sudo ask`, and tool surface are deferred.

## Provider Support (MVP)

Anthropic only, via the Messages API:
```
POST https://api.anthropic.com/v1/messages
```

OpenAI and other providers are deferred.

## Acceptance Condition

```bash
$ model add anthropic --key $ANTHROPIC_KEY
$ model default anthropic/claude-sonnet-4-5
$ echo hello
hello
$ ask "What did I just run?"
You ran `echo hello`, which printed "hello" to the terminal.
```

The AI response appears on stdout and is appended to the transcript.
All existing tests continue to pass.
