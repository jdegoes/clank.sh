---
title: ask — Realized Design
date: 2026-03-07
author: agent
---

## Overview

This document records the `ask` implementation as actually built. It supersedes
any prior approved design for this area (none existed).

---

## What Was Built

`ask` is a shell-internal command intercepted in `run_repl()`. It reads the
current transcript from `ClankShell`, constructs an Anthropic Messages API
request, sends it via `clank-http`'s `HttpClient`, prints the response to
stdout, and appends a `TranscriptEntry::AiResponse` to the transcript.

---

## New Module: `clank/src/ask.rs`

All ask logic is isolated in a dedicated module. Public API:

```rust
pub struct AskInvocation {
    pub prompt: String,
    pub fresh: bool,
    pub model_override: Option<String>,
}

pub enum AskError {
    NoModelConfigured,
    NoApiKey(String),
    Http(#[from] HttpError),
    UnexpectedResponse(String),
    ParseError(String),
    Config(String),
}

pub fn build_user_content(transcript_context: &str, prompt: &str) -> String
pub fn build_request_body(model: &str, user_content: &str) -> String
pub fn strip_provider_prefix(model: &str) -> &str
pub fn extract_response_text(body: &str) -> Result<String, AskError>
pub async fn execute(invocation, transcript_context, http) -> Result<String, AskError>
```

---

## Anthropic Messages API Request

```json
POST https://api.anthropic.com/v1/messages

Headers:
  x-api-key: <api_key>
  anthropic-version: 2023-06-01
  Content-Type: application/json

Body:
{
  "model": "claude-sonnet-4-5",
  "max_tokens": 4096,
  "system": "You are an AI agent operating a bash-compatible shell...",
  "messages": [
    { "role": "user", "content": "<transcript>\n\n<prompt>" }
  ]
}
```

Model name: provider prefix stripped (`anthropic/claude-sonnet-4-5` →
`claude-sonnet-4-5`). The Anthropic API accepts short names without date suffix.

Response: `content[0].text` — only `type: "text"` blocks handled for MVP.
API-level errors (`{"error": {...}}`) surfaced as `AskError::UnexpectedResponse`.

---

## Context Construction

| Mode | User message content |
|---|---|
| Default | `transcript.format_for_model()` + `\n\n` + prompt |
| `--fresh` | prompt only — no transcript |

The transcript is formatted with semantic labels (`[input]`, `[output]`, etc.)
via `format_for_model()`, giving the model unambiguous signal about entry kinds.

---

## `ClankShell::run_ask`

```rust
pub async fn run_ask(
    &mut self,
    input: &str,
    http: &Arc<dyn HttpClient>,
) -> Result<String, ask::AskError>
```

1. Parse `AskInvocation::parse(input)` — flags and prompt
2. Build transcript context (empty if `--fresh`)
3. Call `ask::execute()` — loads config, resolves model + key, POSTs
4. On success: `transcript.push_ai_response(&response)`
5. Return response text

---

## `run_repl` Signature

```rust
pub async fn run_repl(mut shell: ClankShell, http: Arc<dyn HttpClient>)
```

`ask` is intercepted with:

```rust
s if s == "ask" || s.starts_with("ask ") => {
    match shell.run_ask(s, &http).await {
        Ok(response) => println!("{response}"),
        Err(e) => eprintln!("clank: ask: {e}"),
    }
}
```

---

## `main.rs`

```rust
let http: Arc<dyn HttpClient> = Arc::new(
    NativeHttpClient::new().expect("failed to build HTTP client")
);
run_repl(shell, http).await;
```

---

## MVP Flags

| Input | Behaviour |
|---|---|
| `ask "prompt"` | Use transcript as context |
| `ask --fresh "prompt"` | No transcript context |
| `ask --no-transcript "prompt"` | Alias for `--fresh` |
| `ask --model <m> "prompt"` | Override default model |

---

## Error Handling

| Error | Message shown |
|---|---|
| No model configured | "no default model configured — run: model add ..." |
| No API key | "no API key for provider '...' — run: model add ..." |
| HTTP failure | "HTTP request failed: ..." |
| API error response | "unexpected API response: API error: ..." |
| Bad parse | "failed to parse ask invocation: ..." |

All errors printed to stderr via `eprintln!`. The REPL continues — a failed
`ask` does not terminate the session.

---

## End-to-End MVP Flow

```bash
$ model add anthropic --key $ANTHROPIC_KEY
$ model default anthropic/claude-sonnet-4-5
$ echo hello
hello
$ ask "What did I just run?"
You ran `echo hello`, which printed "hello" to the terminal.
```

---

## Test Coverage

| Layer | Count | What |
|---|---|---|
| Unit — `ask.rs` | 14 | AskInvocation::parse (7), request construction (3), response extraction (3), strip_provider_prefix |
| Integration — `tests/ask.rs` | 3 | response recorded in transcript, --fresh excludes transcript, default includes transcript |

**Total: 49 lib unit tests + 3 ask integration tests, all passing. Clippy clean.**

---

## Deviations from the Approved Plan

- The plan said to add an integration test against a `mockito` server. The
  actual implementation uses stub `HttpClient` implementations in `tests/ask.rs`
  — simpler, faster, and no HTTP needed to verify transcript recording and
  `--fresh` behaviour. The `clank-http` crate already has the real `mockito`
  tests for actual HTTP POST behaviour.
- Tests that require config file I/O (setting `HOME`) were moved to
  `tests/ask.rs` (integration test process) to avoid `HOME` env var races with
  `clank-config` unit tests running in the same process.
