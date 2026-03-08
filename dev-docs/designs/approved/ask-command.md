---
title: "ask command design"
date: 2026-03-08
author: agent
---

## Scope

Design and implementation record for the `ask` command.  Covers the original
intent, decisions made during implementation, and where the realized code
diverged from the plan.

---

## Goals

- Implement `ask` as a brush-core builtin with `Subprocess` scope.
- Read the current transcript as context by default; suppress it with `--fresh`.
- Accept a prompt from positional arguments and/or piped stdin.
- Call the configured provider via `clank-provider` and print the response.
- Record the response as an `AiResponse` transcript entry in correct order.

## Non-goals

- Tool calling.
- `--json` structured output.
- `ask repl`.
- `--model` flag to override the configured model.
- Streaming responses.
- The system prompt at `/proc/clank/system-prompt`.

---

## Crate structure

```
clank-ask/
  Cargo.toml
  src/
    lib.rs  — AskBuiltin, argument parsing, prompt assembly, provider call,
              pending-response cell, ask_registration()
```

Dependencies: `brush-core`, `clank-http`, `clank-provider`, `clank-transcript`,
`tokio` (rt, rt-multi-thread, io-util).

---

## Registration

`clank_ask::ask_registration()` is called from `clank_core::default_options()`
alongside `context_registration()`.  `ask` is added to `MANIFEST_REGISTRY` in
`clank-builtins` with scope `Subprocess`.

---

## Execution flow

1. Skip `args[0]` (brush-core passes the command name as the first argument).
2. Parse remaining args: collect positional words as the prompt; detect
   `--fresh` / `--no-transcript`; reject unknown flags with exit 2.
3. If stdin is not a TTY, read it and append to the prompt (newline-separated).
4. If the resulting prompt is empty, write usage to stderr and exit 2.
5. Build messages:
   - `Role::System`: orientation prompt.
   - If transcript context enabled (default) and transcript is non-empty:
     `Role::User` = `<transcript entries>\n\n---\n\n<prompt>`.
   - If `--fresh` or transcript is empty: `Role::User` = `<prompt>` only.
6. Call `provider_from_config(Arc::new(NativeHttpClient::new()))`.
7. Call `provider.complete(&messages)` via `block_in_place`.
8. On success: print response to stdout; store in `PENDING_RESPONSE` cell.
9. `run_statement` in `clank-core` reads `take_pending_response()` after
   recording the `Command` entry and pushes `AiResponse` — see deviation note.

---

## AiResponse recording — pending-response cell

**Divergence from plan** — the plan said `AskBuiltin` records `AiResponse`
directly.  This produces incorrect ordering: `AiResponse` before `Command`,
because `run_statement` records `Command` only *after* `execute` returns.

The realized approach: `ask` stores the response text in a process-global
`OnceLock<Mutex<Option<String>>>` cell (`PENDING_RESPONSE`).  After `execute`
returns, `run_statement` calls `clank_ask::take_pending_response()` and, if
Some, pushes `TranscriptEntry::ai_response(...)` immediately after the
`Command` entry.  This produces the correct order:

```
Command("ask hello")
AiResponse("...")
```

`take_pending_response()` is a public function so `clank-core` can call it
without depending on `clank-transcript` for this purpose.

---

## `is_inspection_command` extension

`ask` and `ask <args>` are added to `is_inspection_command` in `clank-core`.
This prevents the response text (which is captured from stdout) from being
recorded as an `Output` entry on top of the `AiResponse` entry.

---

## Exit code mapping

| Condition | Exit |
|---|---|
| Success | 0 |
| No prompt provided | 2 |
| Unknown flag | 2 |
| `NotConfigured` | 2 |
| `Status(401)` | 2 |
| `Transport` | 4 |
| `Status(n != 401)` | 4 |
| `Parse` | 4 |

---

## Test coverage

### Integration tests (`clank-core/tests/ask.rs`, 6 tests)

In-process mock Ollama server; same pattern as `summarize.rs`:

- `ask_records_ai_response_not_output` — Command + AiResponse, no Output entry.
- `ask_includes_transcript_context_and_records_ai_response` — transcript context sent to provider; correct entry ordering.
- `ask_fresh_omits_transcript_from_request_but_records_response` — `--fresh` omits transcript from request body; AiResponse still recorded.
- `ask_interactive_records_ai_response_not_output` — same invariants in `run_interactive` mode.
- `ask_no_prompt_exits_2_records_command` — no prompt → exit 2, Command entry only.
- `ask_exits_zero_on_success` — exit code 0 against mock.

### Acceptance tests (`clank-acceptance/cases/builtins/ask.yaml`, 3 tests)

- `no_prompt_exits_2` — no args → exit 2, stderr non-empty.
- `unknown_flag_exits_2` — unrecognised flag → exit 2.
- `not_configured_exits_2` — `HOME=/tmp`, no `ask.toml` → exit 2.

---

## Deviations from plan

| Area | Plan | As built | Reason |
|---|---|---|---|
| AiResponse recording site | Inside `AskBuiltin::execute` directly | Via `PENDING_RESPONSE` cell read by `run_statement` | Direct push records `AiResponse` before `Command` because `run_statement` records `Command` only after `execute` returns; the cell defers the push to preserve correct ordering |
| brush-core args convention | Not mentioned | `args.next()` skips `args[0]` (command name) | brush-core passes the command name as the first argument; omitting the skip caused the command name to be treated as the prompt |
