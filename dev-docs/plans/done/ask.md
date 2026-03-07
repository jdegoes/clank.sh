---
title: Implement ask — AI Model Integration
date: 2026-03-07
author: agent
issue: dev-docs/issues/open/ask.md
research:
  - dev-docs/research/ask.md
designs:
  - dev-docs/designs/approved/transcript.md
  - dev-docs/designs/approved/http-client.md
---

## Summary

Implement `ask` as a shell-internal command intercepted in `run_repl()`.
`ask` reads the current transcript, constructs an Anthropic Messages API
request, sends it via `clank-http`, prints the response to stdout, and
appends it to the transcript as `TranscriptEntry::AiResponse`.

## Developer Feedback

- `ask` is shell-internal — intercepted in `run_repl()`, not a Brush builtin.
- MVP: Anthropic only. OpenAI and others are deferred.
- MVP flags: (none), `--fresh`, `--model <m>`.
- `Arc<dyn HttpClient>` passed into `run_repl()` — constructed in `main.rs`.
- System prompt is minimal for MVP.

## Architecture

```
main.rs
  → build_shell()           → ClankShell
  → NativeHttpClient::new() → Arc<dyn HttpClient>
  → run_repl(shell, http)

run_repl() intercepts:
  s if s.starts_with("ask") → shell.run_ask(s, &http).await
```

`ClankShell::run_ask()` owns the ask logic:
1. Parse flags and prompt from the input string
2. Build context: transcript + optional piped stdin + user prompt
3. Call `clank_config::load_config()` for model + API key
4. POST to Anthropic API via `http.post_json()`
5. Parse response, print to stdout
6. Append `TranscriptEntry::AiResponse` to transcript

## New Module: `clank/src/ask.rs`

All ask logic lives in a dedicated module to keep `lib.rs` clean:

```rust
/// Parsed `ask` invocation from the REPL input line.
pub struct AskInvocation {
    /// The user's prompt text.
    pub prompt: String,
    /// If true, send only the prompt — no transcript context.
    pub fresh: bool,
    /// Model override. If None, use the configured default.
    pub model_override: Option<String>,
}

impl AskInvocation {
    /// Parse from a raw REPL input string (e.g. `ask --fresh "hello"`).
    pub fn parse(input: &str) -> Result<Self, AskError>
}
```

## Anthropic API Request

```json
{
  "model": "claude-sonnet-4-5",
  "max_tokens": 4096,
  "system": "You are an AI agent operating a bash-compatible shell called clank.sh. The transcript shows the session history. Respond helpfully and concisely.",
  "messages": [
    {
      "role": "user",
      "content": "<transcript>\n\n<prompt>"
    }
  ]
}
```

Model name: strip provider prefix (`anthropic/claude-sonnet-4-5` →
`claude-sonnet-4-5`).

## Error Type

```rust
#[derive(Debug, thiserror::Error)]
pub enum AskError {
    #[error("no default model configured — run: model add <provider> --key <key>")]
    NoModelConfigured,
    #[error("no API key for provider '{0}' — run: model add {0} --key <key>")]
    NoApiKey(String),
    #[error("HTTP request failed: {0}")]
    Http(#[from] clank_http::HttpError),
    #[error("unexpected API response: {0}")]
    UnexpectedResponse(String),
    #[error("failed to parse ask invocation: {0}")]
    ParseError(String),
}
```

## `run_repl` Changes

`run_repl` gains an `Arc<dyn HttpClient>` parameter. `main.rs` constructs
`NativeHttpClient::new()` and passes it in:

```rust
pub async fn run_repl(mut shell: ClankShell, http: Arc<dyn HttpClient>)
```

The `ask` intercept in the REPL:

```rust
s if s.starts_with("ask") => {
    match shell.run_ask(s, &http).await {
        Ok(response) => {
            println!("{response}");
            // transcript already updated inside run_ask
        }
        Err(e) => eprintln!("clank: ask: {e}"),
    }
}
```

## `ClankShell::run_ask`

```rust
pub async fn run_ask(
    &mut self,
    input: &str,
    http: &Arc<dyn HttpClient>,
) -> Result<String, AskError>
```

1. Parse `AskInvocation::parse(input)`
2. Load config, resolve model name and API key
3. Build context string (transcript or empty if `--fresh`)
4. Construct JSON request body
5. Call `http.post_json(url, headers, body).await`
6. Parse response JSON, extract `content[0].text`
7. Append to transcript as `AiResponse`
8. Return the response text

## `main.rs` Changes

```rust
#[tokio::main]
async fn main() {
    let shell = clank::build_shell().await;
    let http = Arc::new(
        clank_http::NativeHttpClient::new()
            .expect("failed to build HTTP client")
    );
    clank::run_repl(shell, http).await;
}
```

## New Dependencies

`clank/Cargo.toml`:
```toml
clank-http   = { path = "../clank-http" }
serde_json   = "1"
```

## Acceptance Tests

1. `cargo test` passes — all existing tests still green.
2. Unit tests for `AskInvocation::parse`: plain prompt, `--fresh`, `--model`.
3. Unit test for request body construction (no real HTTP call).
4. Integration test: `run_ask` against a `mockito` server returns a response
   and appends it to the transcript.
5. `cargo clippy --all-targets -- -D warnings` passes.
6. Manual end-to-end: `ask "What is 2+2?"` returns a model response.

## Tasks

- [ ] Add `clank-http` and `serde_json` to `clank/Cargo.toml`
- [ ] Create `clank/src/ask.rs` with `AskInvocation`, `AskError`, request
      construction, and response parsing
- [ ] Add unit tests in `ask.rs` for `AskInvocation::parse` and request body
      construction
- [ ] Add `run_ask()` method to `ClankShell` in `lib.rs`
- [ ] Update `run_repl()` signature to take `Arc<dyn HttpClient>`
- [ ] Intercept `ask` in `run_repl()` REPL loop
- [ ] Update `main.rs` to construct `NativeHttpClient` and pass to `run_repl()`
- [ ] Add integration test for `run_ask` against mockito server
- [ ] Verify all acceptance tests pass
