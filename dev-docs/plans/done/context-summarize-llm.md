---
title: "Implement context summarize with Ollama and OpenRouter providers"
date: 2026-03-07
author: agent
issue: dev-docs/issues/open/context-summarize-llm.md
research:
  - dev-docs/research/ollama-openrouter-api.md
designs: []
---

## Summary

Implement `context summarize` as a functioning shell builtin that calls a configured LLM provider (Ollama or OpenRouter) with the current transcript and prints the resulting summary to stdout. This requires: adding a `clank-provider` crate with a provider trait and two concrete implementations, extending `clank-http` with a POST method, defining provider configuration in `~/.config/ask/ask.toml`, and wiring the provider into the `context summarize` subcommand handler.

No existing approved design covers the provider layer. A proposed design is written alongside this plan and noted below; implementation proceeds in parallel pending human approval of the design.

**Proposed design:** `dev-docs/designs/proposed/provider-layer.md` (to be written as part of this plan's task 1).

---

## Design decisions and developer feedback

The following decisions were made in consultation with the developer before writing this plan:

**Provider crate location:** A new `clank-provider` crate. Clean separation of concerns: provider abstractions, request/response types, and concrete implementations live in one place, decoupled from `clank-builtins` and `clank-http`.

**Provider configuration:** `~/.config/ask/ask.toml`, matching the README specification. Flat TOML schema for the initial scope (see research doc for schema). The `toml` crate is required as a new dependency.

**Output format for `context summarize`:** Plain text only. The `--json` flag is deferred to the full `ask` command.

**Initial providers:** Both Ollama and OpenRouter from the start. Ollama for local dev (no auth), OpenRouter for production use. The config file selects which one is active.

---

## Architecture

### New crate: `clank-provider`

```
clank-provider/
  Cargo.toml
  src/
    lib.rs          # Provider trait + config types
    config.rs       # ask.toml parsing (toml + serde)
    ollama.rs       # Ollama provider implementation
    openrouter.rs   # OpenRouter provider implementation
```

**Provider trait:**

```rust
pub trait Provider: Send + Sync {
    fn complete(
        &self,
        messages: &[Message],
    ) -> impl Future<Output = Result<String, ProviderError>> + Send;
}

pub struct Message {
    pub role: Role,
    pub content: String,
}

pub enum Role { System, User, Assistant }
```

`complete()` returns the model's text response as a plain `String`. Error variants: `Transport`, `Status(u16)`, `Auth`, `Parse`, `NotConfigured`.

**Config (`~/.config/ask/ask.toml`):**

```toml
provider = "ollama"         # or "openrouter"
model = "llama3.2"
base_url = "http://localhost:11434"   # used only for ollama; ignored for openrouter
openrouter_api_key = "sk-or-..."      # used only for openrouter; must be read-only, never logged
```

The config module reads this file on demand (not cached at startup — allows the user to change config without restarting the shell). Missing `ask.toml` or missing required keys for the selected provider returns `ProviderError::NotConfigured` with a helpful message.

### Extension to `clank-http`

Add `post()` to `HttpClient`:

```rust
fn post(
    &self,
    url: &str,
    headers: &[(&str, &str)],
    body: &[u8],
) -> impl Future<Output = Result<HttpResponse, HttpError>> + Send;
```

`NativeHttpClient` implements this via `reqwest::Client::post()`.

### Wiring into `clank-builtins`

`context summarize` is currently an unknown subcommand (exits 2). It will be changed to:

1. Read the transcript from `clank_transcript::global()`.
2. Format the transcript as a single user message (with a summarization system prompt).
3. Instantiate a provider from config via `clank_provider::provider_from_config()`.
4. Call `provider.complete()`.
5. Print the result to stdout.
6. Exit 0 on success; exit 4 on transport/status error; exit 2 on config error (with message to stderr).

`clank-builtins` will gain a dependency on `clank-provider`.

### Dependency graph after this plan

```
clank-shell → clank-core → clank-builtins → clank-provider → clank-http
                                        ↘                  ↗
                                         clank-transcript
```

---

## New dependencies

| Crate | Version | Purpose | Why no existing crate satisfies |
|---|---|---|---|
| `serde` | `1` (derive feature) | Serialize/deserialize request structs and TOML config | `clank-http` uses reqwest which brings `serde` as a transitive dep, but it is not directly declared in any workspace crate's `Cargo.toml` as a direct dependency with `derive`. Needed explicitly for struct derives. |
| `serde_json` | `1` | Serialize request bodies and deserialize API responses | Same as above — transitive only, not explicitly declared. Needed as a direct dep in `clank-provider`. |
| `toml` | `0.8` | Parse `~/.config/ask/ask.toml` | No TOML parser exists in the workspace. |

All three are pure Rust, widely used, and have no native OS dependencies. WASM-compatible.

These dependencies must be explicitly approved before being added. This plan records the rationale; the human's approval of the plan constitutes approval of these three crates.

---

## Acceptance tests

New YAML test cases under `clank-acceptance/cases/builtins/`:

### `context-summarize-not-configured.yaml`

```yaml
name: context summarize exits 4 when no provider is configured
args: ["-c", "echo hello; context summarize"]
env:
  HOME: /tmp/no-such-home
expect_exit: 4
expect_stderr_contains: "not configured"
```

*(Exit 4 = remote call failed / not configured. The exact exit code for a missing config is discussed below.)*

### `context-summarize-missing-subcommand-still-exits-2.yaml`

```yaml
name: context with unknown subcommand still exits 2
args: ["-c", "context unknownsubcmd"]
expect_exit: 2
```

Note: Live LLM calls are not testable in the acceptance harness without a running Ollama or real API key. The acceptance tests cover the error/configuration paths only. Integration tests in `clank-provider` cover the provider logic with mock HTTP responses.

---

## Exit code for missing/invalid configuration

The README exit code table has no dedicated code for "not configured". The closest is:

- `4` — Remote call failed (HTTP error or connection failure). A missing config prevents the remote call; treating it as a connection failure is defensible.
- `2` — Invalid usage / bad arguments. Could be interpreted as "the command cannot run as configured".

Decision: use `2` for configuration errors (missing config file, missing key for the selected provider). These are user-fixable setup problems, not remote failures. Exit `4` is reserved for errors that happen at the HTTP layer after configuration succeeds. A message is printed to stderr explaining what is missing.

---

## Tasks

- [ ] Write proposed design doc `dev-docs/designs/proposed/provider-layer.md`
- [ ] Extend `clank-http`: add `post()` method to `HttpClient` trait and implement in `NativeHttpClient`
- [ ] Add `clank-provider` crate to workspace: `Cargo.toml`, `src/lib.rs` with `Provider` trait, `Message`, `Role`, `ProviderError`
- [ ] Implement `config.rs` in `clank-provider`: parse `~/.config/ask/ask.toml`, return `ProviderConfig`; return `ProviderError::NotConfigured` when file or required fields are absent
- [ ] Implement `ollama.rs` in `clank-provider`: `OllamaProvider` using `POST /api/chat` with `stream: false`; parse `.message.content` from response
- [ ] Implement `openrouter.rs` in `clank-provider`: `OpenRouterProvider` using `POST /api/v1/chat/completions`; set `Authorization: Bearer <key>` and `HTTP-Referer: https://clank.sh`; parse `.choices[0].message.content`
- [ ] Add `provider_from_config()` factory function in `clank-provider/src/lib.rs` that reads config and returns the appropriate `Arc<dyn Provider>`
- [ ] Add `clank-provider` unit tests: config parsing (valid/missing/incomplete), Ollama response parsing, OpenRouter response parsing (using mock JSON strings, no HTTP)
- [ ] Update `clank-builtins`: add `clank-provider` dependency; implement `context summarize` handler
- [ ] Add acceptance test cases: `context-summarize-not-configured.yaml` (exits 2)
- [ ] Run `cargo test --workspace` and `cargo clippy --workspace --tests -- -D warnings` — all pass
- [ ] Run `cargo fmt --all --check` — passes
- [ ] Write realized design doc `dev-docs/designs/proposed/provider-layer.md` once all tests pass
