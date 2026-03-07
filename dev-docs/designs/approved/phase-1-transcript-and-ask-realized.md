---
title: "Phase 1: Transcript and ask — Realized Design"
date: 2026-03-06
author: agent
realized_design: true
supersedes: "dev-docs/plans/approved/phase-1-transcript-and-ask.md"
---

# Phase 1: Transcript and `ask` — Realized Design

## Overview

This document records the design as actually implemented after Phase 1 and the subsequent
deviation remediation. It supersedes the Phase 1 plan as the reference for future work. The
plan remains as permanent record of intent.

---

## Crate structure changes

| Crate | Change |
|---|---|
| `clank-shell` | Added `transcript.rs`, `context_process.rs`; updated `shell.rs`, `builtins.rs`, `process.rs` |
| `clank-ask` | Fully implemented: `ask_process.rs`, `config.rs`, `model_process.rs`, `provider/mod.rs`, `provider/anthropic.rs` |
| `clank-manifest` | `ManifestRegistry` with `GLOBAL_REGISTRY`, `expose_to_model` field, `init_global_registry()` |
| `clank` (binary) | `processes.rs` with `AskProcess`, `ModelProcess` adapters; `main.rs` wires HTTP + transcript |

---

## Transcript (`clank-shell/src/transcript.rs`)

### Data model

```rust
pub enum EntryKind { Command, Output, AiResponse }

pub struct Entry {
    pub kind: EntryKind,
    pub timestamp: SystemTime,
    pub text: String,
}

pub struct Transcript {
    entries: Vec<Entry>,
    token_budget: usize,       // default: 100_000 (approx tokens)
}
```

Token estimation: `text.len().div_ceil(4)` — approximate chars-per-token.

### API

| Method | Behaviour |
|---|---|
| `append(kind, text, redacted)` | Appends entry; silently ignores empty/whitespace-only text and redacted entries |
| `window()` | Returns slice of entries within token budget, counted from newest backward |
| `clear()` | Discards all entries |
| `trim(n)` | Drops oldest `n` entries |
| `format_for_model()` | Renders window with role prefixes (`$ `, `[assistant] `, bare output) |
| `format_full()` | Renders all entries regardless of budget; used by `context show` |
| `entries()` | Returns full `&[Entry]` for inspection |
| `len()` / `is_empty()` | Entry count |

### Recording behaviour

`ClankShell::run_line()`:
1. Appends the command text as `EntryKind::Command` before execution.
2. For `Subprocess`-scoped commands only: redirects stdout to a tempfile, executes, reads
   the file, tees it to real stdout, appends as `EntryKind::Output`.
3. `shell-internal` and `parent-shell` commands write directly to real stdout — their output
   is not captured or re-recorded (prevents `context show` from duplicating itself).

Output capture uses `tempfile::NamedTempFile`. A pipe was attempted but deadlocked because
Brush clones file descriptors internally — cloned pipe writers stay open beyond `run_string`,
preventing EOF. The tempfile approach avoids this entirely.

AI responses are appended as `EntryKind::AiResponse` by `AskProcess` after receiving the
model's reply.

---

## Session transcript in `ClankShell`

`ClankShell` holds `Arc<RwLock<Transcript>>` and exposes:
- `transcript()` → `Arc<RwLock<Transcript>>`
- `shell_id()` → `u64` (monotonically increasing, per-instance)
- `with_http(transcript, http)` → full constructor
- `with_transcript(transcript)` → convenience (uses `NativeHttpClient`)
- `new()` → convenience (fresh transcript + `NativeHttpClient`)

The shell ID is used to key the global dispatch table so multiple `ClankShell` instances
(e.g. in tests) do not share state.

---

## Dispatch table architecture

`CommandExecuteFunc` in brush-core is a bare fn pointer — closures cannot capture state.
The dispatch table is a global `RwLock<HashMap<(shell_id, name), Arc<dyn Process>>>`.

A thread-local `ACTIVE_SHELL_ID` is set before each `run_string` call. The bare fn pointer
`dispatch_builtin` reads this to look up the correct shell's processes.

A parallel global `RwLock<HashMap<shell_id, Arc<RwLock<Transcript>>>>` stores transcripts
so `ContextProcess` and `AskProcess` (registered in the dispatch table) can access them.

Public API in `clank-shell`:
- `register_command(shell_id, name, process)` — used by `main.rs` to register real impls
- `deregister_command(shell_id, name)` — for future `grease remove`
- `set_active_shell(id)` — called by `run_line` before each execution

---

## `ContextProcess` (`clank-shell/src/context_process.rs`)

Registered as a real `Process` impl for `context`. Holds `Arc<RwLock<Transcript>>` and
`Arc<dyn HttpClient>`.

| Subcommand | Behaviour |
|---|---|
| `context show` | Writes `format_full()` to stdout; output not re-recorded |
| `context clear` | Clears all transcript entries; exit 0 |
| `context summarize` | Calls Anthropic API with full transcript; writes response to stdout; exits 1 if no config |
| `context trim <n>` | Drops oldest `n` entries; exits 2 on bad argument |
| (none / unknown) | Writes usage to stderr; exits 2 |

`context summarize` loads config from `~/.config/ask/ask.toml` via an inline config helper
(duplicated from `clank-ask` — acceptable for Phase 1; to be deduplicated in Phase 5).

---

## `AskConfig` (`clank-ask/src/config.rs`)

Config file: `~/.config/ask/ask.toml` (path via `dirs-next::config_dir()`).

```toml
default_model = "anthropic/claude-sonnet-4-5"

[providers.anthropic]
api_key = "sk-ant-..."
```

Deserialized via `serde` + `toml`. `load()` returns `ConfigError` if absent or malformed.
`load_or_default()` returns empty config on `NotFound`, errors on read/parse failures.

`resolve_model(explicit)` → explicit > config default > hardcoded fallback.
`api_key(provider)` → `Option<&str>` from provider map.

---

## `ModelProvider` trait and `AnthropicProvider` (`clank-ask/src/provider/`)

```rust
#[async_trait]
pub trait ModelProvider: Send + Sync {
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse, ProviderError>;
}
```

`ProviderError` maps to exit codes: `Timeout` → 3, `RemoteCallFailed` → 4,
`NotConfigured` → 1, `Other` → 1.

`AnthropicProvider` calls `POST /v1/messages` with:
- `x-api-key`, `anthropic-version: 2023-06-01`
- `max_tokens: 4096`
- Provider prefix stripped from model name (`"anthropic/claude-sonnet-4-5"` → `"claude-sonnet-4-5"`)
- Multi-block content responses concatenated

`base_url` is overridable for tests.

---

## `run_ask` function (`clank-ask/src/ask_process.rs`)

Pure function; all I/O is explicit parameters. Signature:

```rust
pub async fn run_ask(
    argv: &[String],
    piped_stdin: Vec<u8>,
    http: Arc<dyn HttpClient>,
    transcript_text: String,
    cwd: Option<&str>,
    config_override: Option<AskConfig>,
) -> (String, Vec<u8>, i32)   // (stdout, stderr, exit_code)
```

### Flag parsing

Hand-rolled (no `clap` dependency in Phase 1):
- Positional args joined as prompt
- `--model <name>`, `--json`, `--fresh`/`--no-transcript`, `--inherit`
- Unknown flags → exit 2

### Message construction

The transcript is embedded in the system prompt (not as fake conversation turns):

```
system_prompt = base_system_prompt + "\n\n## Session transcript\n" + transcript_text
```

Piped stdin is prepended to the user prompt:

```
user_message = "[Supplementary input]\n{stdin}\n\n{prompt}"   # if piped
user_message = "{prompt}"                                        # otherwise
```

The messages array always contains exactly one `User` message. No fabricated `Assistant`
role messages are ever sent.

`--fresh` omits the transcript section from the system prompt entirely.

### Exit codes

| Condition | Code |
|---|---|
| Success | 0 |
| Bad args / missing prompt | 2 |
| Model timeout | 3 |
| HTTP / connection error | 4 |
| `--json` but response is not valid JSON | 6 |
| No API key / config error | 1 |

On exit 6: raw model response is emitted to stderr; stdout is empty.

---

## `run_model` function (`clank-ask/src/model_process.rs`)

`model list` — reads config, lists providers and their key status. All other subcommands
(`add`, `remove`, `default`, `info`) are stubs returning exit 1. Deferred to Phase 5.

---

## `AskProcess` and `ModelProcess` adapters (`clank/src/processes.rs`)

Live in the `clank` binary crate — the only crate that depends on both `clank-shell` and
`clank-ask`. Implement `clank_shell::process::Process`.

`AskProcess::run`:
1. Reads transcript window via `self.transcript.read().format_for_model()`
2. Calls `ctx.io.read_piped_stdin()` — reads only if `OpenFile::PipeReader`; returns `None`
   for terminal stdin (never blocks)
3. Calls `run_ask(...)` with `None` config override (loads real config)
4. Writes stdout to `ctx.io.write_stdout`; appends as `AiResponse` to transcript
5. Writes stderr to `ctx.io.write_stderr`
6. Returns `ProcessResult::success()` on exit 0, `failure(n)` otherwise

---

## `ManifestRegistry` (`clank-manifest/src/lib.rs`)

`CommandManifest` gained `expose_to_model: bool` field (default `false`).

`prompt-user` is registered with `expose_to_model = true`, `execution_scope = ShellInternal`.

`subprocess_commands()` returns entries where `execution_scope == Subprocess || expose_to_model`.
This is the tool surface passed to the model by `ask`.

A `GLOBAL_REGISTRY: LazyLock<RwLock<ManifestRegistry>>` singleton is initialised at shell
startup via `init_global_registry()`. Used by `ClankShell::run_line` to determine whether
a command's output should be captured into the transcript.

---

## Output capture scoping

Only `Subprocess`-scoped commands have their stdout captured into the transcript.
`shell-internal` commands (`context`, `jobs`, `alias`, etc.) write directly to real stdout
without capture — this prevents inspection commands like `context show` from re-recording
themselves into the transcript.

The determination is made by looking up the command name in `GLOBAL_REGISTRY` at the start
of `run_line`. Unknown commands default to `is_subprocess = true` (capture by default).

---

## Piped stdin detection

`ProcessIo::read_piped_stdin()` pattern-matches on `brush_core::openfiles::OpenFile`:

```rust
match &mut self.stdin {
    OpenFile::PipeReader(_) => { /* drain and return Some(bytes) */ }
    _ => Ok(None),   // terminal stdin — never block
}
```

`AskProcess` calls this and passes the bytes as `piped_stdin` to `run_ask`. If `None`,
an empty `Vec` is used.

---

## Testing infrastructure additions

| Location | What was added |
|---|---|
| `clank-shell/src/transcript.rs` | 11 unit tests covering all `Transcript` methods |
| `clank-shell/tests/context.rs` | 9 integration tests for all `context` subcommands |
| `clank-shell/tests/transcript_capture.rs` | 4 integration tests: output capture, command recording, multi-command |
| `clank-ask/src/ask_process.rs` | 9 unit tests: flags, transcript inclusion, piped stdin, JSON contract |
| `clank-ask/src/provider/anthropic.rs` | 5 unit tests: success, timeout, HTTP error, request format, multi-block |
| `clank-ask/src/config.rs` | 5 unit tests: deserialisation, model resolution |
| `clank-ask/src/provider/mod.rs` | 3 unit tests: exit code mapping |
| `clank-ask/src/model_process.rs` | 4 unit tests: list, no subcommand, unknown, stubs |
| `clank-manifest/src/lib.rs` | 5 unit tests including `test_prompt_user_in_tool_surface` |
| `clank/tests/ask.rs` | 7 system tests covering no-config, bad args, context commands, model list |
| `clank/tests/fixtures/ask/` | 4 golden fixtures: no-config, context show empty, context clear, model list |

---

## Known limitations carried forward

- **`context summarize` config loading** is duplicated from `clank-ask`. Both use the same
  `~/.config/ask/ask.toml` but load it independently. Deduplicate in Phase 5.
- **`model add/remove/default/info`** are stubs. Full implementation deferred to Phase 5.
- **`ask repl`** deferred to Phase 4.
- **Automatic transcript compaction** deferred to Phase 5.
- **`/proc/clank/system-prompt`** virtual file deferred to Phase 2 (requires VFS).
- **Piped stdin system-level test** — verified at unit level with `MockHttpClient`. A true
  binary-level pipe test (`echo ... | clank ask "..."`) requires the shell's stdin pipe
  to be wired correctly end-to-end; deferred to Phase 2 process model work.
