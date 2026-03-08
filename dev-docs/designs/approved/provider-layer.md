---
title: "Provider layer design"
date: 2026-03-07
author: agent
---

## Scope

Design and implementation record for the `clank-provider` crate, the `post()`
extension to `clank-http`, and the `context summarize` builtin.  Covers the
original intent, the decisions made during implementation, and where the
realized code diverged from the initial design.

---

## Goals

- Define a minimal abstraction over LLM API backends.
- Implement Ollama (local, unauthenticated) and OpenRouter (remote, Bearer auth) as the first two providers.
- Read provider configuration from `~/.config/ask/ask.toml` at call time (not cached at startup).
- Wire `context summarize` to call the configured provider and print the result to stdout.
- Keep all `#[cfg(target_arch)]` guards in `clank-http`; `clank-provider` is target-agnostic.

## Non-goals

- Streaming responses (deferred to `ask` command).
- Tool calling.
- The `ask` command itself.
- `model list/add/remove/default` commands.
- Automatic transcript compaction.
- Any provider beyond Ollama and OpenRouter.

---

## `clank-http` extension

### `HttpClient::post()`

Added to the trait alongside the existing `get()`:

```rust
fn post(
    &self,
    url: &str,
    headers: &[(&str, &str)],
    body: &[u8],
) -> impl Future<Output = Result<HttpResponse, HttpError>> + Send;
```

Headers are caller-supplied key-value pairs. No headers are injected
automatically. `NativeHttpClient` implements this via `reqwest::Client::post()`.
`WasiHttpClient` has a `todo!()` stub consistent with the existing `get()` stub.

`HttpError` gains no new variants; transport errors and non-2xx status codes are
already covered.

---

## `clank-provider` crate

### Crate structure

```
clank-provider/
  Cargo.toml
  src/
    lib.rs          — AnyProvider<H> enum, Message, Role, ProviderError, provider_from_config()
    config.rs       — ProviderConfig, load_config(), validate_config()
    ollama.rs       — OllamaProvider<H>
    openrouter.rs   — OpenRouterProvider<H>
```

### Dependencies

| Crate | Version | Purpose |
|---|---|---|
| `clank-http` | path | HTTP POST calls |
| `serde` | `1` (derive) | Struct serialization/deserialization |
| `serde_json` | `1` | JSON request bodies and response parsing |
| `toml` | `0.8` | `ask.toml` config file parsing |
| `tokio` | `1` (rt) | Runtime handle access |

### Static dispatch

**Divergence from initial design** — see deviations section.

All provider types are generic over `H: HttpClient`, holding `Arc<H>`. No
`dyn HttpClient` is used anywhere. The `AnyProvider<H>` enum handles the
Ollama vs OpenRouter fork at runtime without requiring object-safe async
traits.

```rust
pub enum AnyProvider<H> {
    Ollama(OllamaProvider<H>),
    OpenRouter(OpenRouterProvider<H>),
}

impl<H: HttpClient> AnyProvider<H> {
    pub async fn complete(&self, messages: &[Message]) -> Result<String, ProviderError>;
}

pub fn provider_from_config<H: HttpClient>(
    http: Arc<H>,
) -> Result<AnyProvider<H>, ProviderError>;
```

### `ProviderError`

```rust
pub enum ProviderError {
    NotConfigured(String),  // missing ask.toml or required fields → exit 2
    Transport(String),      // connection refused, DNS failure    → exit 4
    Status(u16),            // non-success HTTP status            → exit 4 (401 → exit 2)
    Parse(String),          // response body parse failure        → exit 4
}
```

Implements `Display` and `std::error::Error`. The `Display` impl renders
user-facing messages directly; callers do not need to format separately.

### Configuration (`config.rs`)

Reads `~/.config/ask/ask.toml` on each call. Home directory resolved via
`$HOME` env var — no `dirs` crate dependency.

Flat TOML schema:

```toml
provider = "ollama"                      # or "openrouter"
model = "llama3.2"
base_url = "http://localhost:11434"      # optional; Ollama only; defaults to localhost:11434
openrouter_api_key = "sk-or-..."         # required when provider = "openrouter"
```

Validation: unknown provider name → `NotConfigured`. Missing
`openrouter_api_key` when provider is `openrouter` → `NotConfigured`. The API
key is never included in error messages.

### Ollama provider (`ollama.rs`)

Endpoint: `POST {base_url}/api/chat`

Request:
```json
{ "model": "...", "messages": [...], "stream": false }
```

Response parsed at: `.message.content`

Error mapping: transport error → `Transport`, non-2xx → `Status(code)`, parse
failure → `Parse`, empty content string → `Parse("empty response from Ollama")`.

### OpenRouter provider (`openrouter.rs`)

Endpoint: `POST https://openrouter.ai/api/v1/chat/completions`

Fixed headers: `Content-Type: application/json`, `Authorization: Bearer <key>`,
`HTTP-Referer: https://clank.sh`, `X-OpenRouter-Title: clank.sh`.

Request:
```json
{ "model": "...", "messages": [...], "stream": false }
```

Response parsed at: `.choices[0].message.content` (nullable — `None` maps to
`Parse`).

Error mapping: same as Ollama.

---

## `clank-builtins` changes

### `context summarize` handler

`summarize_transcript` is a private helper returning `ExecutionResult` directly
— **divergence from initial design**, see deviations section.

Execution order:
1. Call `provider_from_config()` — config errors exit 2 immediately, before reading the transcript.
2. Read and format the transcript as plain text.
3. If the transcript is empty, print `(transcript is empty)` and exit 0.
4. Build system + user messages.
5. Call `provider.complete(&messages)` via `block_in_place` — see async note below.
6. On success: print summary to stdout, exit 0.
7. On `Status(401)`: write auth failure message to stderr, exit 2.
8. On any other error: write error to stderr, exit 4.

### Async in a synchronous builtin

`ContextBuiltin::execute` is called from brush-core's synchronous builtin
dispatch.  The provider call is async.  The initial design proposed
`Handle::current().block_on(...)`.  In practice this panics with "Cannot start
a runtime from within a runtime" when the calling thread is already driving
tokio tasks (both the multi-thread production runtime and the integration test
runtime hit this).

The correct API is `tokio::task::block_in_place`:

```rust
tokio::task::block_in_place(|| {
    tokio::runtime::Handle::current().block_on(provider.complete(&messages))
})
```

`block_in_place` yields the current thread to the tokio scheduler for the
duration, allowing other tasks to make progress.  It requires the
`rt-multi-thread` tokio feature, added to `clank-builtins`.  Integration tests
that exercise this path use `#[tokio::test(flavor = "multi_thread")]`.

### Exit code mapping

| Condition | Exit |
|---|---|
| Success | 0 |
| Empty transcript | 0 |
| `NotConfigured` | 2 |
| `Status(401)` | 2 |
| `Transport` | 4 |
| `Status(n != 401)` | 4 |
| `Parse` | 4 |

---

## Test coverage

### `clank-provider` unit tests (15 tests)

- `config`: valid Ollama config, base URL default, valid OpenRouter config, missing API key error, unknown provider error, missing required field error.
- `ollama`: request serialization, successful response parse, empty content detection, malformed response rejection.
- `openrouter`: request serialization, successful response parse, null content handling, empty choices array, malformed response rejection.

### Integration tests (`clank-core/tests/summarize.rs`, 5 tests)

Exercise the transcript non-recording invariant end-to-end using an in-process
mock Ollama server (tokio TCP listener, no external dependency):

- `context_summarize_output_not_recorded_in_transcript` — core invariant: Command entry recorded, no Output entry for summary text.
- `context_summarize_then_show_neither_records_output` — neither inspection command leaves an Output entry, even in sequence.
- `context_show_output_not_recorded` — `context show` with non-empty output still produces no Output entry.
- `context_summarize_empty_transcript_records_only_command_entry` — only Command entry, no Output for `(transcript is empty)` message.
- `context_summarize_interactive_output_not_recorded` — same invariant holds in `run_interactive` mode.

### Acceptance tests (`clank-acceptance/cases/builtins/context-summarize.yaml`)

- `summarize_not_configured_exits_2`: `HOME=/tmp` (no `ask.toml`) → exit 2, stderr non-empty.
- `unknown_subcommand_still_exits_2`: `context unknownsubcmd` → exit 2.

---

## Deviations from initial design

| Area | Initially proposed | As built | Reason |
|---|---|---|---|
| HTTP dispatch | `Arc<dyn HttpClient>` | `Arc<H>` generic over `H: HttpClient` | `impl Future` in trait methods makes `HttpClient` non-dyn-compatible on stable Rust; generics are cleaner and zero-cost |
| `summarize_transcript` return type | `Result<ExecutionResult, brush_core::Error>` | `ExecutionResult` directly | The function handles all its own errors internally and never returns `Err`; `Result<_, brush_core::Error>` on a standalone function triggers `clippy::result_large_err` and the `allow` would be dishonest |
| Async bridge | `Handle::current().block_on(...)` | `block_in_place(|| Handle::current().block_on(...))` | Plain `block_on` panics when the calling thread is already inside a tokio runtime; `block_in_place` is the correct API for sync→async bridging on the multi-thread runtime |
| Home directory resolution | `dirs` crate mentioned as an option | `std::env::var("HOME")` directly | Avoids an extra dependency; `$HOME` is sufficient for both production and test use |
