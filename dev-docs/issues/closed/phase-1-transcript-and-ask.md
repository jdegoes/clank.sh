---
title: "Phase 1: Transcript and `ask` — core AI integration"
date: 2026-03-06
author: agent
---

# Phase 1: Transcript and `ask`

## Problem

The shell exists but has no AI integration. There is no mechanism to invoke a model, no transcript
for the model to read, and no `context` management.

## Capability Gap

- No transcript data structure — no record of session history.
- `ask` command does not exist.
- `context` builtin does not exist.
- No model provider configuration or HTTP calls to model APIs.
- No stdout/stderr discipline.
- No `--json` output contract.
- Exit codes are not fully enforced.

## Deliverables

The shell maintains a sliding-window transcript of everything rendered to the terminal. `ask`
invokes a configured AI model using the transcript as context. The response is rendered and
appended to the transcript. Basic AI workflows are possible.

Concretely:
- Transcript type: append, redact, read window, clear, trim
- Transcript written to on every command invocation and every output rendered
- `ask` subprocess: reads transcript window, calls model HTTP API via `HttpClient`, streams/buffers
  response, writes to stdout, appends to transcript
- `ask` flags: `--model`, `--json`, `--fresh`/`--no-transcript`, `--inherit`
- `context` builtin: `show`, `clear`, `summarize`, `trim` — output to stdout, not re-recorded
- `/proc/clank/system-prompt` virtual file (initial version — static content, not yet dynamic)
- Model config: `~/.config/ask/ask.toml` (default model, provider API key)
- `model` command stub: `model list`, `model add`, `model default`
- Stdout/stderr discipline enforced throughout
- Exit codes 0, 1, 2, 3, 4, 6 enforced correctly
- `ask "hello"` works end-to-end on both native and WASM targets
- Piped stdin supplementation: `echo "context" | ask "question"`

## Open Questions Requiring Design

- Initial system prompt content and structure (see gap in
  `dev-docs/research/spec-analysis-and-implementation-gaps.md`).
- Transcript compaction: triggering condition and summarization model call. For Phase 1, compaction
  can be manual-only (`context summarize` + `context clear`); automatic compaction deferred.

## Out of Scope

`ask repl`, process table, `prompt-user`, authorization, virtual filesystem (except
`/proc/clank/system-prompt` as a stub), MCP, Golem, `grease`, tab completion.
