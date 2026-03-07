---
title: "Plan: Phase 1 — Transcript and `ask`"
date: 2026-03-06
author: agent
issue: "dev-docs/issues/open/phase-1-transcript-and-ask.md"
research:
  - "dev-docs/research/spec-analysis-and-implementation-gaps.md"
designs:
  - "dev-docs/designs/approved/workspace-and-crate-structure-realized.md"
---

# Plan: Phase 1 — Transcript and `ask`

## Originating Issue

`dev-docs/issues/open/phase-1-transcript-and-ask.md` — the shell has no AI integration; no
transcript, no `ask`, no `context` management.

## Research Consulted

- `dev-docs/research/spec-analysis-and-implementation-gaps.md` — identifies the system prompt
  and transcript compaction as the two open design questions for this phase; confirms automatic
  compaction is out of scope and manual-only compaction is acceptable.

## Design Referenced

- `dev-docs/designs/approved/workspace-and-crate-structure-realized.md` — current crate
  layout; Phase 1 adds real implementations to `clank-shell` (transcript, `context` builtin),
  `clank-ask` (`ask` process, `model` command), and extends `clank-manifest` with an initial
  manifest registry.

## Developer Feedback

**System prompt structure (2026-03-06):** The proposed initial system prompt structure is
acceptable as a starting point for iteration. The spec explicitly defers the exact content as
"a prompt engineering problem whose solution will evolve" — the plan's draft is consistent with
the spec's stated concerns (environment, filesystem map, tool semantics, `prompt-user` emphasis,
parameter collection) and need not be final before implementation begins.

**Model provider scope (2026-03-06):** Anthropic only for Phase 1. The `ModelProvider` trait
makes adding further providers mechanical; no second provider is in scope for this phase.

## Open Design Questions

Two questions require developer input before implementation begins.

**1. Initial system prompt content and structure.**

The README defers this as "a prompt engineering problem whose solution will evolve." For Phase
1 the system prompt must be good enough for `ask` to work usefully, but it does not need to be
final. The proposed initial structure:

```
You are clank, an AI-native shell running on Linux. You help the user with tasks by
executing commands and interpreting their output.

## Environment
- Shell: bash-compatible (clank.sh / Brush)
- Working directory: <cwd>
- Available tools: every subprocess-scoped command on $PATH

## Important tools
- `prompt-user`: pause and ask the human a question. Use it whenever you need
  clarification or approval before proceeding.

## Constraints
- You run as a subprocess. You cannot change the parent shell's working directory
  or environment variables directly.
- Prefer composable pipelines over monolithic scripts.
```

This is assembled dynamically from shell state on each `ask` invocation (cwd, available
commands). **Question: Is this structure acceptable for Phase 1, or are there specific
additions or structural changes you want before we start?**

**2. Model provider for Phase 1.**

`ask` must call a real model API. Anthropic (Claude) is the natural first implementation
given the project context, using the Messages API with streaming. **Question: Should Phase 1
implement only Anthropic, or also add OpenAI as a second provider? Recommendation: Anthropic
only — a clean provider trait makes adding OpenAI mechanical in Phase 2 or 5.**

## Approach

### Transcript (`clank-shell`)

`Transcript` is a first-class owned value inside `ClankShell`. It is a `Vec<Entry>` where
each `Entry` carries a kind tag (command, output, ai-response), a timestamp, and the raw text.
Redaction is applied at append time: entries governed by a `redaction-rules` manifest entry
are never stored. The sliding window is a view over the `Vec` bounded by a configurable token
budget (default 100k tokens); entries are counted approximately (chars / 4).

`ClankShell::run_interactive()` is updated to append every command typed and every line of
output produced to the transcript. This is the only place transcript writes happen for Phase
1 — there is no compaction and no automatic management.

`context` is registered as a real `shell-internal` `Process` implementation (replacing the
stub) backed by a shared `Arc<RwLock<Transcript>>` passed to `ClankShell` at construction.

### `ask` process (`clank-ask`)

`ask` is a `subprocess`-scoped `Process` implementation. When dispatched via Brush:

1. Reads the transcript window from `Arc<RwLock<Transcript>>` (injected at startup).
2. Reads its own stdin for supplementary piped input.
3. Builds the request: system prompt + transcript window + piped stdin (on a separate channel).
4. Calls the model provider via `Arc<dyn HttpClient>`.
5. Streams/buffers the response; writes to its stdout handle.
6. Appends the AI response to the transcript.
7. Exits with the correct exit code.

The provider is selected from config (`~/.config/ask/ask.toml`). The config is read once at
`ask` startup (not at shell startup) so changes take effect without restarting the shell.

`ask` uses `clap` for argument parsing. Flags: positional prompt string, `--model`, `--json`,
`--fresh`/`--no-transcript`, `--inherit`. All non-result output (tool traces, warnings) goes
to stderr; the model response goes to stdout.

### Config (`clank-ask`)

`~/.config/ask/ask.toml`:
```toml
default_model = "anthropic/claude-sonnet-4-5"

[providers.anthropic]
api_key = "sk-ant-..."
```

Config is read with `serde` + `toml`. If the config file is absent or has no API key, `ask`
exits `1` with an informative message explaining what to configure. No silent failures.

### Provider trait (`clank-ask`)

```rust
#[async_trait]
pub trait ModelProvider: Send + Sync {
    async fn complete(
        &self,
        request: CompletionRequest,
    ) -> Result<CompletionStream, ProviderError>;
}

pub struct CompletionRequest {
    pub model: String,
    pub system_prompt: String,
    pub messages: Vec<Message>,  // transcript window + piped stdin
    pub stream: bool,
}
```

`AnthropicProvider` is the only implementation in Phase 1, calling the Anthropic Messages API
via `Arc<dyn HttpClient>`. The streaming response is collected and written to the process's
stdout handle as it arrives. Non-streaming fallback used when streaming is not available.

### `model` command (`clank-ask`)

Stub with correct subcommand structure: `model list`, `model add`, `model remove`,
`model default`, `model info`. All subcommands print "not yet implemented" in Phase 1 except
`model list` which reads the config and lists configured providers and their default models.

### System prompt assembly

The system prompt is assembled at `ask` invocation time (not at shell startup) from:
- A static header (role, environment description)
- Current working directory (from shell env)
- Available `subprocess`-scoped commands (from the manifest registry)

For Phase 1 the manifest registry is minimal (only the commands registered at startup). The
system prompt is a plain string — no template engine. Written to a string, passed to the
provider.

`/proc/clank/system-prompt` is a stub: reading it returns the string "system prompt not yet
available" until Phase 2 adds the virtual filesystem. It is not a blocking concern for `ask`
to work end-to-end.

### Transcript injection into dispatch

`ClankShell::new()` now takes `Arc<RwLock<Transcript>>` as a parameter (or creates one
internally). The transcript is passed as a dependency to the `AskProcess` and `ContextProcess`
implementations registered in the global dispatch table.

Since `CommandExecuteFunc` is a bare fn pointer, both `AskProcess` and `ContextProcess` are
stored in the global `RwLock<DispatchTable>` with their captured `Arc<RwLock<Transcript>>` —
exactly as `StubProcess` is stored today. No change to the dispatch mechanism is needed.

---

## Tasks

- [ ] **`clank-shell`: `Transcript` type**
  - [ ] Define `Entry` struct: `kind: EntryKind`, `timestamp: SystemTime`, `text: String`
  - [ ] Define `EntryKind` enum: `Command`, `Output`, `AiResponse`
  - [ ] Define `Transcript` struct: `entries: Vec<Entry>`, `token_budget: usize`
  - [ ] Implement `append(&mut self, kind, text)` — applies redaction (Phase 1: no redaction
        rules yet, all entries accepted)
  - [ ] Implement `window(&self) -> &[Entry]` — returns all entries within token budget
        (approximate: `text.len() / 4` token estimate per entry)
  - [ ] Implement `clear(&mut self)` — discards all entries
  - [ ] Implement `trim(&mut self, n: usize)` — drops oldest `n` entries
  - [ ] Implement `format_for_model(&self) -> String` — renders window as a single string
        suitable for inclusion in the model request
  - [ ] Unit tests for all `Transcript` methods (see acceptance tests)

- [ ] **`clank-shell`: wire transcript into interactive loop**
  - [ ] `ClankShell` holds `Arc<RwLock<Transcript>>`
  - [ ] `run_interactive()` appends each command to transcript before execution
  - [ ] `run_interactive()` captures stdout/stderr of each command and appends output to
        transcript after execution
  - [ ] Unit tests: transcript entries created correctly after `run_line()` calls

- [ ] **`clank-shell`: `ContextProcess` — real `context` builtin**
  - [ ] Replace stub with real implementation
  - [ ] `context show` — writes formatted transcript to process stdout; not re-recorded
  - [ ] `context clear` — clears transcript
  - [ ] `context summarize` — calls model for a summary, writes to process stdout; not
        re-recorded. For Phase 1 this uses the same provider as `ask`; if no provider is
        configured, prints an informative error and exits `1`
  - [ ] `context trim <n>` — drops oldest `n` entries
  - [ ] Uses `clap` for subcommand parsing; exit `2` on invalid usage
  - [ ] Crate integration tests for all four subcommands via `ClankShell::run_line()`

- [ ] **`clank-ask`: config**
  - [ ] Define `AskConfig` struct with `serde` + `toml`: `default_model`, `providers` map
  - [ ] `AskConfig::load()` — reads `~/.config/ask/ask.toml`; returns informative error if
        absent or malformed
  - [ ] Unit tests: deserialisation, missing file, missing API key

- [ ] **`clank-ask`: `ModelProvider` trait and `AnthropicProvider`**
  - [ ] Define `ModelProvider` trait, `CompletionRequest`, `CompletionStream`, `ProviderError`
  - [ ] `ProviderError` maps to exit codes: timeout → `3`, HTTP error → `4`
  - [ ] `AnthropicProvider::new(api_key, Arc<dyn HttpClient>)`
  - [ ] Implement Anthropic Messages API call (POST `/v1/messages`): build request JSON,
        send via `HttpClient`, parse response
  - [ ] Streaming support: server-sent events parsed from response body; yield tokens as
        they arrive
  - [ ] Non-streaming fallback for when `stream: false`
  - [ ] Unit tests with `MockHttpClient`: successful response, timeout (exit 3), HTTP error
        (exit 4), malformed JSON in response

- [ ] **`clank-ask`: `AskProcess`**
  - [ ] `clap`-based argument parsing: prompt string (positional), `--model`, `--json`,
        `--fresh`/`--no-transcript`, `--inherit`
  - [ ] Reads transcript window from `Arc<RwLock<Transcript>>` (unless `--fresh`)
  - [ ] Reads piped stdin (drains `ProcessIo::stdin`)
  - [ ] Assembles system prompt from cwd + manifest registry
  - [ ] Calls provider; streams response to `ProcessIo::stdout`
  - [ ] Appends AI response to transcript
  - [ ] `--json`: validates response is JSON; exits `6` with raw response to stderr if not
  - [ ] All side-channel output (warnings, errors) to `ProcessIo::stderr`
  - [ ] Correct exit codes: `0` success, `1` general, `2` bad args, `3` timeout, `4` HTTP
        error, `6` bad JSON
  - [ ] Crate integration tests with `MockHttpClient`

- [ ] **`clank-ask`: `ModelProcess` — `model` command**
  - [ ] `model list` — reads config, prints configured providers and models
  - [ ] `model add`, `model remove`, `model default`, `model info` — stub with "not yet
        implemented" message, exit `1`
  - [ ] Crate integration tests for `model list`

- [ ] **`clank-shell`: register real processes**
  - [ ] Replace stub registrations for `ask`, `context`, `model` in dispatch table with real
        `AskProcess`, `ContextProcess`, `ModelProcess`
  - [ ] `AskProcess` and `ContextProcess` receive `Arc<RwLock<Transcript>>` at construction

- [ ] **`clank-manifest`: initial registry**
  - [ ] Add `ManifestRegistry`: a `HashMap<String, CommandManifest>`, queryable by name
  - [ ] Populate at shell startup with manifests for all registered commands (at minimum:
        `ask`, `context`, `model`, core commands as stubs)
  - [ ] `ManifestRegistry::subprocess_commands()` — returns manifests for all
        `subprocess`-scoped commands; used by `ask` to assemble the system prompt tool list
  - [ ] Unit tests for registry lookup and `subprocess_commands()` filter

- [ ] **Exit codes and stdout/stderr discipline — audit**
  - [ ] Verify `run_interactive()` never writes to stderr except on genuine errors
  - [ ] Verify all clank-produced messages (not model output) go to stderr
  - [ ] Verify exit codes 0, 1, 2, 3, 4, 6 are returned correctly by `ask`
  - [ ] Add system test: `ask` with no config → exit `1`, informative stderr message

---

## Acceptance Tests

All of the following must pass before this phase is considered complete.

### Unit tests (Level 1)

| Test | Crate | File |
|---|---|---|
| `test_transcript_append_stores_entry` | `clank-shell` | `process/transcript.rs` |
| `test_transcript_window_respects_token_budget` | `clank-shell` | `process/transcript.rs` |
| `test_transcript_clear_empties_entries` | `clank-shell` | `process/transcript.rs` |
| `test_transcript_trim_drops_oldest_n` | `clank-shell` | `process/transcript.rs` |
| `test_transcript_format_for_model_roundtrip` | `clank-shell` | `process/transcript.rs` |
| `test_ask_config_load_valid` | `clank-ask` | `config.rs` |
| `test_ask_config_missing_file_returns_error` | `clank-ask` | `config.rs` |
| `test_provider_error_timeout_maps_exit_3` | `clank-ask` | `provider/mod.rs` |
| `test_provider_error_http_maps_exit_4` | `clank-ask` | `provider/mod.rs` |
| `test_anthropic_builds_correct_request_json` | `clank-ask` | `provider/anthropic.rs` |
| `test_anthropic_parses_streaming_response` | `clank-ask` | `provider/anthropic.rs` |
| `test_anthropic_mock_success` | `clank-ask` | `provider/anthropic.rs` |
| `test_anthropic_mock_timeout` | `clank-ask` | `provider/anthropic.rs` |
| `test_anthropic_mock_http_error` | `clank-ask` | `provider/anthropic.rs` |
| `test_manifest_registry_lookup` | `clank-manifest` | `lib.rs` |
| `test_manifest_registry_subprocess_filter` | `clank-manifest` | `lib.rs` |

### Crate integration tests (Level 2)

| Test | Crate | File |
|---|---|---|
| `test_context_show_prints_transcript` | `clank-shell` | `tests/context.rs` |
| `test_context_clear_empties_transcript` | `clank-shell` | `tests/context.rs` |
| `test_context_trim_drops_n_entries` | `clank-shell` | `tests/context.rs` |
| `test_context_show_not_re_recorded` | `clank-shell` | `tests/context.rs` |
| `test_ask_fresh_ignores_transcript` | `clank-ask` | `tests/ask.rs` |
| `test_ask_inherit_includes_transcript` | `clank-ask` | `tests/ask.rs` |
| `test_ask_json_valid_exits_0` | `clank-ask` | `tests/ask.rs` |
| `test_ask_json_invalid_exits_6_stderr_has_raw` | `clank-ask` | `tests/ask.rs` |
| `test_ask_piped_stdin_appended_to_context` | `clank-ask` | `tests/ask.rs` |
| `test_model_list_reads_config` | `clank-ask` | `tests/model.rs` |

### System tests (Level 3) — new file `crates/clank/tests/ask.rs`

| Test | Description |
|---|---|
| `test_ask_no_config_exits_1_with_message` | No `~/.config/ask/ask.toml` → exit 1, stderr explains what to configure |
| `test_ask_hello_returns_response` | End-to-end `ask "hello"` with real API key (skipped in CI if key absent) |
| `test_ask_json_flag_valid_response` | `ask --json "..."` → stdout is valid JSON |
| `test_ask_json_flag_invalid_response` | Provider returns non-JSON → exit 6, raw on stderr |
| `test_ask_fresh_flag` | `ask --fresh "..."` → model receives no transcript context |
| `test_context_show_and_clear` | `echo hi && context show` contains "hi"; `context clear && context show` is empty |

### Golden file fixtures — new directory `tests/fixtures/ask/`

| Fixture | What it pins |
|---|---|
| `ask_no_config.toml` | Exit 1 + exact stderr message when config absent |
| `ask_bad_args.toml` | Exit 2 + usage message for invalid flag |
| `context_show_empty.toml` | `context show` on empty transcript |
| `context_clear.toml` | `context clear` succeeds silently |
| `model_list_no_config.toml` | `model list` with no config → informative message |

---

## Notes on Implementation Order

1. `Transcript` type and unit tests first — everything else depends on it.
2. Wire transcript into `run_interactive()` — enables integration tests for `context`.
3. `ContextProcess` — validates transcript wiring before adding HTTP complexity.
4. `AskConfig` + `ModelProvider` trait + `MockHttpClient` tests — pure logic, no shell needed.
5. `AnthropicProvider` against real API — validate end-to-end before wiring into dispatch.
6. `AskProcess` — wires transcript, provider, and I/O together.
7. Replace stub registrations + run full suite.
8. Golden fixtures last — generated from stable output with `TRYCMD=overwrite`.

## Notes on Deferred Items

- **`/proc/clank/system-prompt`**: stub only (returns placeholder string). Virtual filesystem
  is Phase 2.
- **Automatic transcript compaction**: deferred to Phase 5. Manual compaction via
  `context summarize && context clear` is sufficient for Phase 1.
- **`context summarize`**: calls the model for a summary. If no API key is configured, exits
  `1` with an informative error. This is acceptable Phase 1 behaviour.
- **`ask repl`**: deferred to Phase 4.
- **`sudo ask`**: authorization model is Phase 2; `sudo` prefix is ignored in Phase 1.
- **OpenAI provider**: deferred. `ModelProvider` trait makes addition mechanical.
- **`model add/remove/default/info`**: stubs in Phase 1; full implementation in Phase 5.
