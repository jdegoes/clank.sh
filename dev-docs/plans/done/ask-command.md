---
title: "Implement the ask command"
date: 2026-03-08
author: agent
issue: dev-docs/issues/open/ask-command.md
research: []
designs:
  - dev-docs/designs/approved/provider-layer.md
---

## Summary

Implement `ask` as a brush-core builtin declared with `Subprocess` scope, housed
in a new `clank-ask` crate.  The command reads the current transcript as context,
accepts a positional prompt argument and optional piped stdin, calls the
configured provider via `clank-provider`, prints the response to stdout, and
records the response as an `AiResponse` transcript entry.

No proposed design doc is required for this feature — the provider layer design
covers all provider-interaction concerns and this plan is self-contained.

---

## Design decisions and developer feedback

**Implementation location:** A new `clank-ask` crate registers `AskBuiltin` with
brush-core.  It is declared `Subprocess` scope in the manifest registry.  Using
a builtin rather than a real executable avoids the need for an actual binary on
`$PATH` while keeping the architecture honest about scope intent.  This matches
the pattern established by `context` (`ShellInternal`) — the execution mechanism
is a builtin, but the manifest scope declaration is authoritative.

**Transcript as context:** All transcript entries are formatted as plain text
(via `display_plain()`) and concatenated into a single `Role::User` message,
preceded by a `Role::System` message.  This is the same formatting used by
`context summarize` and is sufficient for the initial implementation.  Multi-turn
conversation mapping is deferred.

**Argument + stdin combination:** The positional argument is the primary prompt.
If stdin is not a TTY (i.e. content is piped), it is read and appended after the
argument, separated by a newline.  If no argument is provided and stdin is
piped, stdin alone is the prompt.  If neither is provided, `ask` exits 2 with a
usage message.

**AiResponse recording:** `AskBuiltin` pushes an `AiResponse` entry to
`clank_transcript::global()` directly, before returning.  This mirrors how
`context summarize` reads the transcript.  No changes to `clank-core`'s
`run_statement` are needed.

**`--fresh` / `--no-transcript` flag:** When present, the transcript is not
included in the context — the model receives only the prompt.  Both flags are
aliases for the same behaviour.

---

## Architecture

### New crate: `clank-ask`

```
clank-ask/
  Cargo.toml
  src/
    lib.rs     — AskBuiltin, argument parsing, prompt assembly, provider call,
                 AiResponse recording
```

Dependencies: `brush-core`, `clank-provider`, `clank-http`, `clank-transcript`,
`tokio` (rt, rt-multi-thread).

### Registration

`clank-ask` exposes `ask_registration() -> Registration`, called from
`clank-core::default_options()` alongside `context_registration()`.  `ask` is
added to `MANIFEST_REGISTRY` in `clank-builtins` with scope `Subprocess`.

### Execution flow

1. Parse args: collect positional arguments as the prompt string; detect
   `--fresh` / `--no-transcript`.
2. If stdin is not a TTY, read it and append to the prompt (newline-separated).
3. If the resulting prompt is empty, write usage to stderr, exit 2.
4. Build messages:
   - `Role::System`: a brief orientation prompt (where the model is, what it
     should do).
   - If transcript context is enabled (default): `Role::User` containing all
     transcript entries formatted via `display_plain()`, joined by newlines,
     followed by a separator and the prompt.
   - If `--fresh`: `Role::User` containing only the prompt.
5. Instantiate provider via `provider_from_config(Arc::new(NativeHttpClient::new()))`.
6. Call `provider.complete(&messages)` via `block_in_place`.
7. On success: print response to stdout; push `TranscriptEntry::ai_response(response)` to global transcript; exit 0.
8. On `ProviderError::NotConfigured`: write to stderr, exit 2.
9. On `ProviderError::Status(401)`: write auth failure to stderr, exit 2.
10. On any other error: write to stderr, exit 4.

### Transcript recording

`AiResponse` recording happens inside `AskBuiltin::execute`, not in
`run_statement`.  The `is_inspection_command` check in `run_statement` already
prevents `ask`'s stdout from being double-recorded as an `Output` entry — `ask`
must be added to that predicate.

### `run_statement` changes

`is_inspection_command` in `clank-core/src/lib.rs` must be extended to include
`ask` (and `ask --fresh`, etc.) so the response printed to stdout is not also
recorded as an `Output` entry.  The `AiResponse` entry pushed by `AskBuiltin`
is the authoritative record of the exchange.

---

## Exit code mapping

| Condition | Exit |
|---|---|
| Success | 0 |
| No prompt provided | 2 |
| `NotConfigured` | 2 |
| `Status(401)` | 2 |
| `Transport` | 4 |
| `Status(n != 401)` | 4 |
| `Parse` | 4 |

---

## Acceptance tests

New YAML file `clank-acceptance/cases/builtins/ask.yaml`:

- `ask_not_configured_exits_2`: `HOME=/tmp`, `stdin: ask hello` → exit 2, stderr non-empty.
- `ask_no_prompt_exits_2`: `stdin: ask` → exit 2, stderr non-empty.
- `ask_unknown_flag_exits_2`: `stdin: ask --unknown-flag` → exit 2.

Live provider calls are not testable in the acceptance harness.

## Integration tests

New file `clank-core/tests/ask.rs` using the same in-process mock Ollama server
pattern from `clank-core/tests/summarize.rs`:

- `ask_records_ai_response_in_transcript`: after `ask "hello"` succeeds against the mock, transcript contains `Command("ask \"hello\"")` and `AiResponse("<mock response>")` — no `Output` entry for the response text.
- `ask_fresh_flag_omits_transcript_from_context`: with `--fresh`, a seeded transcript is present but the mock receives only the prompt (verifiable via the request body captured by the mock).
- `ask_with_no_transcript_context_still_records_response`: even with `--fresh`, the `AiResponse` is recorded.
- `ask_exit_code_zero_on_success`: exits 0 against the mock.

---

## Tasks

- [ ] Add `clank-ask` crate to workspace: `Cargo.toml`, `src/lib.rs` with `AskBuiltin` stub
- [ ] Register `ask` in `clank-builtins` `MANIFEST_REGISTRY` with scope `Subprocess`
- [ ] Register `ask_registration()` in `clank-core::default_options()`
- [ ] Implement argument parsing in `AskBuiltin`: positional prompt, `--fresh` / `--no-transcript` flag, stdin detection
- [ ] Implement prompt assembly: transcript context (default) and fresh mode
- [ ] Wire provider call via `block_in_place` + `provider_from_config`
- [ ] Print response to stdout and push `AiResponse` entry to global transcript
- [ ] Implement exit code mapping for all `ProviderError` variants
- [ ] Extend `is_inspection_command` in `clank-core` to cover `ask` invocations
- [ ] Add acceptance tests: `clank-acceptance/cases/builtins/ask.yaml`
- [ ] Add integration tests: `clank-core/tests/ask.rs` with mock Ollama server
- [ ] Run `cargo test --workspace`, `cargo clippy --workspace --tests -- -D warnings`, `cargo fmt --all --check` — all pass
- [ ] Write realized design doc `dev-docs/designs/proposed/ask-command.md`
