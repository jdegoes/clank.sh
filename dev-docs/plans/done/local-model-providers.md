---
title: "Plan: Local model providers (Ollama, OpenAI-compatible)"
date: 2026-03-06
author: agent
issue: "dev-docs/issues/open/local-model-providers.md"
research:
  - "dev-docs/research/local-model-providers.md"
designs:
  - "dev-docs/designs/approved/phase-1-transcript-and-ask-realized.md"
---

# Plan: Local Model Providers (Ollama, OpenAI-compatible)

## Originating Issue

`dev-docs/issues/open/local-model-providers.md` — clank requires a cloud API key for every
`ask` invocation; no local inference backend is supported.

## Research Consulted

`dev-docs/research/local-model-providers.md` — surveyed Ollama, llama.cpp server, and LM
Studio. Key conclusions:

- **Two providers, not one.** Ollama has a distinct native wire format (`/api/chat`) and
  no-auth design. llama.cpp, LM Studio, vLLM, and similar tools share the OpenAI
  `/v1/chat/completions` format. Treating these as a single backend with auto-detection
  would be fragile; explicit config is better.
- **`ProviderConfig` needs `base_url`.** A new optional field must be added alongside
  `api_key` to address local servers.
- **`model add` must be partially implemented.** Local providers require no API key, so
  writing config for them is the minimal viable entrypoint into `model add`.
- **No streaming required.** Both backends support `"stream": false`. Clank does not use
  streaming today; none is needed here.

## Design Referenced

`dev-docs/designs/approved/phase-1-transcript-and-ask-realized.md` — describes the
existing `ModelProvider` trait, `AskConfig`, `ProviderConfig`, and `select_provider`
that this plan extends.

---

## Approach

### New files

| File | Content |
|---|---|
| `crates/clank-ask/src/provider/wire.rs` | Shared OpenAI-format request/response structs |
| `crates/clank-ask/src/provider/ollama.rs` | `OllamaProvider` |
| `crates/clank-ask/src/provider/openai_compat.rs` | `OpenAiCompatProvider` |

### Changed files

| File | Change |
|---|---|
| `crates/clank-ask/src/config.rs` | Add `base_url: Option<String>` to `ProviderConfig`; add `AskConfig::base_url()` helper; add `AskConfig::save()`; add `ConfigError::Write` variant |
| `crates/clank-ask/src/provider/mod.rs` | Declare `pub mod wire`, `pub mod ollama`, `pub mod openai_compat` |
| `crates/clank-ask/src/provider/openrouter.rs` | Switch inline wire types to use `wire` module |
| `crates/clank-ask/src/ask_process.rs` | Extend `select_provider` with `"ollama"` and `"openai-compat"` arms |
| `crates/clank-ask/src/model_process.rs` | Implement `model add` for all four providers; update `model list` |

No changes to `clank-shell`, `clank-http`, or `clank-manifest` are required.

---

## Stream A — Config schema (`config.rs`)

### A1. Add `base_url` to `ProviderConfig`

```rust
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProviderConfig {
    pub api_key: Option<String>,
    #[serde(default)]
    pub base_url: Option<String>,
}
```

`#[serde(default)]` preserves backwards compatibility: existing config files with no
`base_url` field deserialise without error, yielding `None`.

`#[derive(Default)]` is required because `model add` uses
`config.providers.entry(provider).or_default()` to upsert entries. Since both fields are
`Option<T>`, the derived default is `ProviderConfig { api_key: None, base_url: None }`.

### A2. Add `AskConfig::base_url()` helper

```rust
/// Get the configured base URL for a provider, if any.
pub fn base_url(&self, provider: &str) -> Option<&str> {
    self.providers.get(provider)?.base_url.as_deref()
}
```

### A3. Add `AskConfig::save()`

`model add` needs to write back to the config file. `AskConfig::save()` serialises to
TOML and writes atomically to prevent corruption on interrupted writes.

```rust
pub fn save(&self) -> Result<(), ConfigError> { ... }
```

**Atomicity:** The temp file is written to the same directory as the target config file
(not to a system temp directory), so the final `rename` is always within the same
filesystem and the operation is atomic on all supported platforms. The procedure is:

1. `std::fs::create_dir_all` on the parent directory.
2. Write serialised TOML to `<config_path>.tmp` in the same directory.
3. `std::fs::rename(<config_path>.tmp, <config_path>)`.

A new `ConfigError::Write { path: PathBuf, source: std::io::Error }` variant is added for
write and rename failures.

### A4. Config write merge semantics

`model add` only writes the fields it was given. Fields already present in an existing
entry are left unchanged. Concretely:

- `model add ollama --url http://new-host` on a config that already has
  `providers.ollama.api_key = "x"` updates only `base_url`; `api_key` is preserved.
- `model add anthropic --key <new-key>` on a config that already has
  `providers.anthropic.base_url = "http://x"` updates only `api_key`; `base_url` is
  preserved.

Implementation:
```rust
let entry = config.providers.entry(provider.to_string()).or_default();
if let Some(url) = new_base_url {
    entry.base_url = Some(url);
}
if let Some(key) = new_api_key {
    entry.api_key = Some(key);
}
```

---

## Stream B — `OllamaProvider`

### Wire format

**Request** — `POST {base_url}/api/chat`:

```json
{
  "model": "llama3.2",
  "messages": [
    { "role": "system", "content": "..." },
    { "role": "user",   "content": "..." }
  ],
  "stream": false
}
```

The model string has the `"ollama/"` prefix stripped before sending.

**Response** — non-streaming:

```json
{ "message": { "role": "assistant", "content": "..." }, "done": true }
```

Extract `response.message.content`.

### Error mapping

| Condition | `ProviderError` |
|---|---|
| Connection refused / DNS failure | `RemoteCallFailed("Ollama is not running at {base_url}. Start it with: ollama serve")` |
| HTTP 404 | `RemoteCallFailed("Model '{model}' not found. Pull it with: ollama pull {model}")` |
| HTTP 5xx | `RemoteCallFailed("Ollama returned {status}: {body}")` |
| Request timeout | `Timeout` |

### Constructor

```rust
pub fn new(base_url: impl Into<String>, http: Arc<dyn HttpClient>) -> Self
```

`base_url` is a concrete `String` (not `Option`). The caller is responsible for resolving
the default. `select_provider` applies the default before calling `new`:

```rust
let base_url = config.base_url("ollama")
    .unwrap_or("http://localhost:11434")
    .to_string();
OllamaProvider::new(base_url, http)
```

This is consistent with the pattern already used by `AnthropicProvider` and
`OpenRouterProvider`, which also take concrete strings. There is no `Option`-accepting
constructor — defaults live at the call site, not inside the constructor.

### Internal types (private to the module)

```rust
#[derive(Serialize)]
struct OllamaChatRequest { model: String, messages: Vec<OllamaMessage>, stream: bool }

#[derive(Serialize, Deserialize)]
struct OllamaMessage { role: String, content: String }

#[derive(Deserialize)]
struct OllamaChatResponse { message: OllamaMessage }
```

---

## Stream C — `OpenAiCompatProvider`

### Wire format

Identical to `OpenRouterProvider` (`/v1/chat/completions`, OpenAI format), with two
differences:

1. The base URL is a required constructor argument; there is no default.
2. The `Authorization: Bearer` header is included only when `api_key` is `Some(s)` where
   `s` is non-empty. Both `None` and `Some("")` omit the header.

The `model` field is set to the portion after `"openai-compat/"`. For single-model
servers (llama-server, LM Studio) the field is ignored; for multi-model servers it
selects the model.

### Constructor

```rust
pub fn new(
    base_url: impl Into<String>,
    api_key: Option<String>,
    http: Arc<dyn HttpClient>,
) -> Self
```

`base_url` is a required `String`. There is no default and no `Option`. If `base_url` is
absent from config, `select_provider` returns an informative error before this constructor
is ever called — the constructor never receives an invalid state. No panics.

### Error mapping

Same as `OpenRouterProvider`: HTTP 401/402 → `NotConfigured`, HTTP 5xx / connection
error → `RemoteCallFailed`, timeout → `Timeout`.

### `select_provider` pre-check for missing `base_url`

`select_provider` validates and resolves `base_url` before constructing either local
provider:

```rust
"openai-compat" => {
    let base_url = config.base_url("openai-compat").ok_or_else(|| {
        "clank: ask: no base_url configured for provider 'openai-compat'\n\
         Run: model add openai-compat --url http://localhost:8080\n".to_string()
    })?;
    let api_key = config.api_key("openai-compat").map(str::to_string);
    return Ok(Box::new(OpenAiCompatProvider::new(base_url.to_string(), api_key, http)));
}
```

The constructor receives a guaranteed-present `base_url`. No internal guard is needed.

---

## Stream B1 — `wire.rs` shared module

`OpenRouterProvider` currently defines its OpenAI-format request/response types inline.
`OpenAiCompatProvider` uses the same wire format. Rather than duplicate these structs,
this plan introduces `crates/clank-ask/src/provider/wire.rs` with the shared types, and
updates `OpenRouterProvider` to import from it.

```rust
// wire.rs — shared OpenAI chat completions request/response types

#[derive(Serialize)]
pub struct ChatRequest<'a> {
    pub model: &'a str,
    pub max_tokens: u32,
    #[serde(skip_serializing_if = "str::is_empty")]
    pub system: &'a str,
    pub messages: Vec<ChatMessage<'a>>,
    pub stream: bool,
}

#[derive(Serialize)]
pub struct ChatMessage<'a> {
    pub role: &'a str,
    pub content: &'a str,
}

#[derive(Deserialize)]
pub struct ChatResponse {
    pub choices: Vec<Choice>,
}

#[derive(Deserialize)]
pub struct Choice {
    pub message: AssistantMessage,
}

#[derive(Deserialize)]
pub struct AssistantMessage {
    pub content: String,
}
```

`OpenRouterProvider`'s existing unit tests are the regression net for this refactor: they
must pass unchanged after the switch to `wire.rs` types. No new tests are needed for the
refactor itself.

---

## Stream D — `select_provider` changes (`ask_process.rs`)

The local-provider arms are inserted before the key-based check, since local providers
require no API key:

```rust
fn select_provider(
    model: &str,
    config: &AskConfig,
    http: Arc<dyn HttpClient>,
) -> Result<Box<dyn ModelProvider>, String> {
    let provider_name = if model.contains('/') {
        model.split('/').next().unwrap_or("anthropic")
    } else {
        "anthropic"
    };

    // Local providers — no API key required; handled before the key-based check.
    match provider_name {
        "ollama" => {
            let base_url = config.base_url("ollama")
                .unwrap_or("http://localhost:11434")
                .to_string();
            return Ok(Box::new(OllamaProvider::new(base_url, http)));
        }
        "openai-compat" => {
            let base_url = config.base_url("openai-compat").ok_or_else(|| {
                "clank: ask: no base_url configured for provider 'openai-compat'\n\
                 Run: model add openai-compat --url http://localhost:8080\n".to_string()
            })?;
            let api_key = config.api_key("openai-compat").map(str::to_string);
            return Ok(Box::new(OpenAiCompatProvider::new(
                base_url.to_string(),
                api_key,
                http,
            )));
        }
        _ => {}
    }

    // Cloud providers — require a direct API key or OpenRouter fallback.
    if let Some(key) = config.api_key(provider_name) {
        return Ok(match provider_name {
            "anthropic" => Box::new(AnthropicProvider::new(key, http)),
            "openrouter" => Box::new(OpenRouterProvider::new(key, http)),
            // Any other provider with a direct key configured also routes through
            // OpenRouter, which accepts all provider/model strings natively.
            _ => Box::new(OpenRouterProvider::new(key, http)),
        });
    }

    // OpenRouter fallback for any model without a direct provider key.
    if let Some(key) = config.api_key("openrouter") {
        return Ok(Box::new(OpenRouterProvider::new(key, http)));
    }

    Err(format!(
        "clank: ask: no API key configured for provider '{provider_name}' or 'openrouter'\n\
         To use Anthropic directly:  add [providers.anthropic] api_key = \"...\" to ~/.config/ask/ask.toml\n\
         To use OpenRouter:          add [providers.openrouter] api_key = \"...\" to ~/.config/ask/ask.toml\n"
    ))
}
```

---

## Stream E — `model add` implementation (`model_process.rs`)

### Subcommands implemented

```
model add ollama [--url <base_url>]
model add openai-compat --url <base_url> [--key <api_key>]
model add anthropic --key <api_key>
model add openrouter --key <api_key>
```

`model add` for any other provider name with `--key` also works (stores a generic
`ProviderConfig { api_key: Some(key), base_url: None }`).

### Flag parsing

`model add` parses its own argv slice with a simple loop — no clap dependency. Flags:

| Flag | Value type | Providers | Required |
|---|---|---|---|
| `--key <value>` | String | `anthropic`, `openrouter`, `openai-compat` | `anthropic`/`openrouter` only |
| `--url <value>` | String | `ollama`, `openai-compat` | `openai-compat` only |

Error cases that return exit 2:
- `model add` with no provider name
- `model add openai-compat` without `--url`
- `model add anthropic` without `--key`
- `model add openrouter` without `--key`
- Unknown flag

### Config write behaviour

1. Load existing config via `AskConfig::load_or_default()`.
2. Get or create the provider entry: `config.providers.entry(provider).or_default()`.
3. Write only the fields supplied by the user; leave other fields unchanged (see merge
   semantics in A4).
4. Call `config.save()`.
5. Print confirmation to stdout: `"Provider '{name}' configured.\n"`

If `save()` fails, print the error to stderr and exit 1.

### `model remove` and `model default`

These remain stubs in this plan. Their scope is narrow enough to defer without blocking
local model usage. The stub message changes from `"not yet implemented"` to
`"not yet implemented (planned)"` to distinguish from hard stubs.

### `model list` update

`model list` currently shows only `api_key` status. After this plan it also shows
`base_url` when present. The exact format:

```
Default model: ollama/llama3.2

Providers:
  anthropic:      api_key configured
  ollama:         base_url=http://localhost:11434
  openai-compat:  base_url=http://localhost:8080, api_key configured
  openrouter:     api_key configured
```

Rules:
- Providers are sorted alphabetically (unchanged from current behaviour).
- If only `api_key` is set: `api_key configured` (or `no API key` if absent).
- If only `base_url` is set: `base_url=<url>`.
- If both are set: `base_url=<url>, api_key configured`.
- If neither is set (should not occur after a `model add`, but possible if hand-edited):
  `no configuration`.

---

## Design decisions

**Why not reuse `OpenRouterProvider` for `openai-compat`?**
The wire format is the same, but `OpenRouterProvider` hardcodes the endpoint URL,
always sends OpenRouter-specific identifying headers (`http-referer`, `x-openrouter-title`),
and assumes a key is always present. A separate `OpenAiCompatProvider` avoids coupling
local-server behaviour to OpenRouter-specific code. The two implementations share
serialisation structs via `wire.rs`.

**Why a shared `wire` module instead of duplicating structs?**
`OpenRouterProvider` already has its own inline OpenAI request/response types. Introducing
`wire.rs` and updating `OpenRouterProvider` to use it eliminates the duplication before it
exists. The refactor is validated by `OpenRouterProvider`'s existing unit tests, which are
the regression net and must pass unchanged.

**Why implement `model add` for all four providers (not just local ones)?**
The config-write code path is identical. Adding `anthropic` and `openrouter` in the same
diff costs a handful of lines and eliminates the state where `model add anthropic --key
<KEY>` still says "not yet implemented" after local providers work. The test matrix is
also cleaner.

**`OllamaProvider::new` takes a concrete `String`, not `Option<String>`.**
Default resolution (`"http://localhost:11434"`) happens at the call site in
`select_provider`, matching the pattern used by `AnthropicProvider` and
`OpenRouterProvider`. Constructors that apply defaults internally make the call-site
behaviour invisible and are harder to test directly.

**`OpenAiCompatProvider::new` takes a concrete `String` for `base_url`.**
`select_provider` validates and resolves `base_url` before constructing the provider,
returning an informative error if it is absent. The constructor therefore always receives
a valid URL and requires no internal guard. No panics, no `Option`, consistent with
project conventions (`AGENTS.md`: "Return `Result<T, E>` throughout. Never panic in
library code.").

**`AskConfig::save()` temp file is in the same directory as the target.**
`std::fs::rename` is only atomic when source and destination are on the same filesystem.
Writing to `<config_path>.tmp` (in the same directory as `ask.toml`) guarantees this
without relying on system temp directory placement.

---

## Acceptance tests

### A: Config schema

| Test | Level | Assertion |
|---|---|---|
| `test_provider_config_base_url_roundtrips` | Unit | `ProviderConfig { base_url: Some("http://x"), api_key: None }` serialises to TOML and deserialises correctly |
| `test_provider_config_no_base_url_deserialises` | Unit | Existing TOML with no `base_url` field deserialises to `base_url: None` (no parse error) |
| `test_ask_config_base_url_helper` | Unit | `config.base_url("ollama")` returns the configured value |
| `test_ask_config_save_roundtrip` | Unit | `save()` then `load()` returns equal config |
| `test_ask_config_save_creates_parent_dir` | Unit | `save()` to a path whose parent doesn't exist creates the directory and writes successfully |
| `test_ask_config_save_atomic_temp_in_same_dir` | Unit | The `.tmp` file is written to the same directory as the config path, not to a system temp dir |
| `test_ask_config_save_preserves_unrelated_fields` | Unit | `save()` of a config with both `api_key` and `base_url` set round-trips both fields intact |

### B: `OllamaProvider`

| Test | Level | Assertion |
|---|---|---|
| `test_ollama_builds_correct_request` | Unit | Given a `CompletionRequest`, the JSON sent to `MockHttpClient` has `model` without the `"ollama/"` prefix, `stream: false`, correct `messages` array |
| `test_ollama_mock_success` | Unit | `MockHttpClient` returning a valid chat response → `CompletionResponse::content` equals expected text |
| `test_ollama_mock_404` | Unit | HTTP 404 → `ProviderError::RemoteCallFailed` containing "not found" and a hint to `ollama pull` |
| `test_ollama_mock_connection_refused` | Unit | Connection error → `ProviderError::RemoteCallFailed` containing "not running" and a hint to `ollama serve` |
| `test_ollama_mock_timeout` | Unit | Timeout error → `ProviderError::Timeout`, exit code 3 |
| `test_ollama_custom_base_url_used_in_request` | Unit | `OllamaProvider::new("http://myhost:11434", http)` sends request to `http://myhost:11434/api/chat` |

### C: `OpenAiCompatProvider`

| Test | Level | Assertion |
|---|---|---|
| `test_openai_compat_omits_auth_header_when_no_key` | Unit | `api_key = None` → no `Authorization` header in request |
| `test_openai_compat_omits_auth_header_when_empty_key` | Unit | `api_key = Some("")` → no `Authorization` header in request |
| `test_openai_compat_includes_auth_header_when_key_set` | Unit | `api_key = Some("sk-x")` → `Authorization: Bearer sk-x` header present |
| `test_openai_compat_strips_provider_prefix_from_model` | Unit | `CompletionRequest { model: "openai-compat/phi4" }` → `model` field in request body is `"phi4"` |
| `test_openai_compat_mock_success` | Unit | Valid response → correct `content` extracted from `choices[0].message.content` |
| `test_openai_compat_mock_http_error` | Unit | HTTP 5xx → `ProviderError::RemoteCallFailed`, exit code 4 |
| `test_openai_compat_mock_timeout` | Unit | Timeout → `ProviderError::Timeout`, exit code 3 |
| `test_openai_compat_omits_openrouter_headers` | Unit | Request does not contain `http-referer` or `x-openrouter-title` headers |

### D: `select_provider`

| Test | Level | Assertion |
|---|---|---|
| `test_select_ollama_no_base_url_uses_default` | Unit | Config has `providers.ollama` with no `base_url` → request goes to `http://localhost:11434/api/chat` |
| `test_select_ollama_custom_base_url` | Unit | `providers.ollama.base_url = "http://myhost:11434"` → request goes to `http://myhost:11434/api/chat` |
| `test_select_openai_compat_missing_url_returns_error` | Unit | `providers.openai-compat` with no `base_url` → `select_provider` returns error containing `"no base_url"` and the `model add` hint |
| `test_select_openai_compat_missing_url_run_ask_exits_1` | Crate integration | `run_ask` with `openai-compat` model and no `base_url` configured → exit 1, stderr contains `"no base_url"` |
| `test_select_openai_compat_with_url` | Unit | `providers.openai-compat.base_url = "http://x:8080"` → request goes to `http://x:8080/v1/chat/completions` |
| `test_select_provider_ollama_routing` | Crate integration | `run_ask` with `--model ollama/llama3.2` and mock config → mock receives request at path `/api/chat` |
| `test_select_provider_openai_compat_routing` | Crate integration | `run_ask` with `--model openai-compat/phi4` and mock config → mock receives request at path `/v1/chat/completions` |

### E: `model add` and `model list`

| Test | Level | Assertion |
|---|---|---|
| `test_model_add_ollama_no_url_flag_writes_default` | Unit | `model add ollama` writes `providers.ollama.base_url = "http://localhost:11434"` |
| `test_model_add_ollama_custom_url` | Unit | `model add ollama --url http://remote:11434` writes the correct `base_url` |
| `test_model_add_ollama_preserves_existing_api_key` | Unit | `model add ollama --url http://x` when entry already has `api_key` → `api_key` is unchanged |
| `test_model_add_openai_compat_no_url_exits_2` | Unit | `model add openai-compat` without `--url` → exit 2, stderr contains usage |
| `test_model_add_openai_compat_with_url` | Unit | `model add openai-compat --url http://x:8080` → writes `base_url`; `api_key` absent from entry |
| `test_model_add_openai_compat_with_url_and_key` | Unit | `model add openai-compat --url http://x:8080 --key sk-x` → writes both `base_url` and `api_key` |
| `test_model_add_anthropic_writes_key` | Unit | `model add anthropic --key sk-ant-x` → writes `api_key`; `base_url` absent from entry |
| `test_model_add_anthropic_no_key_exits_2` | Unit | `model add anthropic` without `--key` → exit 2, stderr contains usage |
| `test_model_add_anthropic_preserves_existing_base_url` | Unit | `model add anthropic --key sk-x` when entry already has `base_url` → `base_url` is unchanged |
| `test_model_add_unknown_flag_exits_2` | Unit | `model add ollama --frobnicate` → exit 2 |
| `test_model_add_no_provider_exits_2` | Unit | `model add` with no args → exit 2 |
| `test_model_list_shows_base_url_only` | Unit | Provider with only `base_url` set → output shows `base_url=<url>`, no `api_key` mention |
| `test_model_list_shows_both_fields` | Unit | Provider with both `base_url` and `api_key` set → output shows `base_url=<url>, api_key configured` |
| `test_model_list_shows_key_only` | Unit | Provider with only `api_key` set → output shows `api_key configured` (unchanged from current) |

### F: Golden fixtures

Three new golden test fixtures in `crates/clank/tests/fixtures/model/`:

| Fixture | stdin | Assertion |
|---|---|---|
| `add_ollama_default.toml` | `model add ollama\n` | Exit 0; stdout is `"Provider 'ollama' configured.\n"` |
| `add_openai_compat_missing_url.toml` | `model add openai-compat\n` | Exit 2; stderr contains usage message with `--url` hint |
| `list_with_local_providers.toml` | `model add ollama\nmodel add openai-compat --url http://localhost:8080\nmodel list\n` | Exit 0; stdout matches expected `model list` output including `base_url` lines |

---

## Tasks

- [ ] **A1** Add `base_url: Option<String>` (with `#[serde(default)]`) and `#[derive(Default)]` to `ProviderConfig`; add `AskConfig::base_url()` helper; add unit tests for roundtrip, backwards compatibility, and both-fields round-trip
- [ ] **A2** Add `ConfigError::Write` variant
- [ ] **A3** Add `AskConfig::save()` with atomic write (temp file in same directory as target) and `create_dir_all`; add unit tests including temp-file-location and parent-dir-creation tests
- [ ] **B1** Create `crates/clank-ask/src/provider/wire.rs` with shared OpenAI-format request/response structs; update `OpenRouterProvider` to import from it; verify all existing `OpenRouterProvider` unit tests still pass
- [ ] **B2** Implement `OllamaProvider` in `provider/ollama.rs` with `new(base_url: String, http)` constructor; add unit tests
- [ ] **C1** Implement `OpenAiCompatProvider` in `provider/openai_compat.rs` with `new(base_url: String, api_key: Option<String>, http)` constructor; add unit tests including both `None` and `Some("")` no-auth cases and the no-OpenRouter-headers test
- [ ] **D1** Extend `select_provider` with `"ollama"` and `"openai-compat"` arms (before key-based check); add unit tests and crate integration tests including the `run_ask`-level missing-URL error test
- [ ] **E1** Implement `model add` flag parsing and selective config write (merge semantics per A4); add unit tests including field-preservation tests
- [ ] **E2** Update `model list` to show `base_url` per the format specified above; add unit tests for all three display cases (key only, URL only, both)
- [ ] **F1** Add three golden fixtures: `add_ollama_default`, `add_openai_compat_missing_url`, `list_with_local_providers`
- [ ] **QG** `cargo test --workspace`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check` all pass
