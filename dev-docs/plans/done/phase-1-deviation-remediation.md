---
title: "Plan: Phase 1 Deviation Remediation"
date: 2026-03-06
author: agent
issue: "dev-docs/issues/open/phase-1-deviations.md"
research: []
designs:
  - "dev-docs/designs/approved/workspace-and-crate-structure-realized.md"
---

# Plan: Phase 1 Deviation Remediation

## Originating Issue

`dev-docs/issues/open/phase-1-deviations.md` — six deviations from spec identified after
Phase 1 implementation. All must be corrected before Phase 1 closeout.

## Approach

The six deviations are addressed in order of severity, with the two high-severity items first.
Each is a targeted fix with a clear before/after and its own acceptance test.

---

## Deviation 1 — Output capture in transcript

**Fix:** Use `tokio::net::unix::pipe::Receiver` to convert the `std::io::PipeReader` into an
async reader. Then use `tokio::join!` to run `shell.run_string()` and the pipe drain
concurrently in the same tokio task. Both futures can make progress — `run_string` writes to
the pipe writer (which closes when it returns), and the drain reads until EOF. No deadlock.

**Location:** `crates/clank-shell/src/shell.rs` — `run_line()`.

**Acceptance:** After `run_line("echo hello")`, `transcript.read().unwrap().entries` contains
an `Output` entry with text `"hello"`. Covered by a new crate integration test in
`crates/clank-shell/tests/transcript_capture.rs`.

---

## Deviation 2 — Piped stdin to `ask`

**Fix:** In `crates/clank/src/processes.rs`, inspect `ctx.io.stdin` to determine whether it
is a `PipeReader`. `brush_core::openfiles::OpenFile` is an enum; the `PipeReader` variant
carries a `std::io::PipeReader`. If that variant is present, read it into `piped`. Otherwise
leave `piped` empty and do not block.

Pattern match on the enum directly — no flag, no heuristic:
```rust
let piped = match ctx.io.stdin {
    OpenFile::PipeReader(r) => { let mut buf = vec![]; r.read_to_end(&mut buf)?; buf }
    _ => vec![],
};
```

**Acceptance:** System test: `echo "supplementary" | ask "what did I pipe?"` — the request
body sent to the mock provider contains `"supplementary"`. Covered by a crate integration
test in `crates/clank/tests/ask.rs` using `MockHttpClient`.

---

## Deviation 3 — `context summarize` calls the model

**Fix:** In `crates/clank-shell/src/context_process.rs`, the `"summarize"` branch calls the
model via the same `HttpClient` and `AskConfig` machinery as `ask`. The model is called with:
- System prompt: a brief instruction to produce a concise summary of a shell transcript
- A single user message: the full transcript text (`format_full()`)

The response is written to stdout. If no API key is configured, emit a clear error to stderr
and exit 1.

**Dependency:** `ContextProcess` needs access to `Arc<dyn HttpClient>`. Update its constructor
to accept one.

**Acceptance:** With a `MockHttpClient` returning a canned summary, `context summarize`
writes that summary to stdout and exits 0. Without a config, exits 1 with an informative
message. Covered by updated crate integration tests in `crates/clank-shell/tests/context.rs`.

---

## Deviation 4 — `ProcessResult::failure` for exit code 0

**Fix:** In `crates/clank/src/processes.rs`, replace `ProcessResult::failure(exit_code)`
with:
```rust
if exit_code == 0 { ProcessResult::success() } else { ProcessResult::failure(exit_code) }
```
Apply to both `AskProcess` and `ModelProcess`.

**Acceptance:** Unit test verifying `AskProcess` returns `ProcessResult::success()` when
`run_ask` returns exit code 0.

---

## Deviation 5 — Remove fake assistant turn from transcript messages

**Fix:** In `crates/clank-ask/src/ask_process.rs`, remove the fabricated `Assistant`
acknowledgement messages. The transcript and piped stdin should be sent as context without
fake turns:

- Include the transcript as a single `User` message labelled clearly, with no fabricated
  `Assistant` reply.
- Include piped stdin as a second `User` message if present.
- Then add the actual user prompt as the final `User` message.

The Anthropic Messages API requires alternating `user`/`assistant` roles. The correct way to
provide background context without a fake reply is to include it in the `system` prompt, or
to prefix the first user message with the context. For Phase 1: prepend the transcript to the
system prompt (it is context the model should have, not a turn in the conversation). Piped
stdin is sent as a prefix to the first user message.

Updated message structure:
```
system_prompt = <base system prompt> + "\n\n## Session transcript\n" + transcript_text
messages = [
    // piped stdin prefix, if any:
    User: "[Supplementary input]\n{stdin_text}\n\nPrompt: {prompt}",
    // OR without piped stdin:
    User: "{prompt}",
]
```

**Acceptance:** Unit test verifying no `Assistant` role messages appear in the request body
when transcript or piped stdin is present. Covered by updated tests in
`crates/clank-ask/src/ask_process.rs`.

---

## Deviation 6 — `prompt-user` included in `ask` tool surface

**Fix:** In `crates/clank-manifest/src/lib.rs`, update `subprocess_commands()` to include
`prompt-user` explicitly despite its `ShellInternal` scope, matching the spec exception. Add
a dedicated method or a flag on `CommandManifest` to mark it as "exposed to model tool
surface despite scope".

Simplest correct implementation: add a `expose_to_model: bool` field to `CommandManifest`
(default `false`), set it `true` for `prompt-user`, and update `subprocess_commands()` to
return all entries where `execution_scope == Subprocess || expose_to_model`.

**Acceptance:** Unit test verifying `subprocess_commands()` includes `prompt-user` but
excludes `cd`, `context`, `jobs`, etc. Updated test in `crates/clank-manifest/src/lib.rs`.

---

## Tasks

- [ ] **Dev 1:** Fix output capture — async pipe drain with `tokio::join!`
  - [ ] In `run_line`: create pipe, set stdout fd on params, `tokio::join!` run_string + async drain
  - [ ] Convert `PipeReader` to `tokio::net::unix::pipe::Receiver` via `from_file_unchecked`
  - [ ] Append drained output as `EntryKind::Output` to transcript after join
  - [ ] Write to real stdout after capture (tee behaviour)
  - [ ] Add unit test: after `run_line("echo hello")`, transcript contains Output entry "hello"
  - [ ] Confirm existing system tests still pass

- [ ] **Dev 2:** Fix piped stdin — detect `OpenFile::PipeReader`
  - [ ] In `AskProcess::run`: pattern-match `ctx.io.stdin` on `OpenFile::PipeReader`
  - [ ] Read and drain only when `PipeReader` variant
  - [ ] Add crate integration test: `echo "supplementary" | ask "..."` sends stdin in request
  - [ ] Update `test_ask_piped_stdin_appended_to_context` accordingly

- [ ] **Dev 3:** Fix `context summarize` — call the model
  - [ ] Add `Arc<dyn HttpClient>` to `ContextProcess` constructor and struct
  - [ ] In `"summarize"` branch: load config, call `AnthropicProvider::complete` with full
        transcript as user message
  - [ ] Write response to stdout (not re-recorded per spec)
  - [ ] If no API key: exit 1 with informative message
  - [ ] Update `ClankShell::with_transcript` to pass `HttpClient` to `ContextProcess`
  - [ ] Add/update integration tests in `context.rs`

- [ ] **Dev 4:** Fix `ProcessResult` — use `success()` for exit code 0
  - [ ] `AskProcess::run`: return `ProcessResult::success()` when exit_code == 0
  - [ ] `ModelProcess::run`: same
  - [ ] Add unit tests verifying correct result type per exit code

- [ ] **Dev 5:** Fix message structure — remove fake assistant turns
  - [ ] Move transcript into system prompt
  - [ ] Merge piped stdin prefix with user prompt in a single message
  - [ ] Remove all fabricated `Assistant` role messages
  - [ ] Update all affected unit tests

- [ ] **Dev 6:** Fix `prompt-user` in tool surface
  - [ ] Add `expose_to_model: bool` field to `CommandManifest` (default `false`)
  - [ ] Set `expose_to_model = true` for `prompt-user` in `populate_defaults()`
  - [ ] Update `subprocess_commands()` to include entries where `expose_to_model` is true
  - [ ] Add unit test: `prompt-user` in `subprocess_commands()`, `context` not in it

- [ ] **Final:** Run full quality gate
  - [ ] `cargo test --workspace` — all tests pass
  - [ ] `cargo clippy --all-targets -- -D warnings` — clean
  - [ ] `cargo fmt --check` — clean
  - [ ] `TRYCMD=overwrite cargo test --test golden` — update affected golden fixtures

---

## Acceptance Tests

| # | Test | Location | Assertion |
|---|---|---|---|
| D1a | `test_output_captured_in_transcript` | `clank-shell/tests/transcript_capture.rs` | After `run_line("echo hello")`, transcript contains `Output` entry with text `"hello"` |
| D1b | `test_output_displayed_to_stdout` | `clank-shell/tests/transcript_capture.rs` | Same `run_line` also produces visible output |
| D2 | `test_ask_piped_stdin_in_request` | `clank/tests/ask.rs` | Request body contains piped text when stdin is a pipe |
| D3a | `test_context_summarize_calls_model` | `clank-shell/tests/context.rs` | With mock provider, `context summarize` writes the mock response to stdout |
| D3b | `test_context_summarize_no_config_exits_1` | `clank-shell/tests/context.rs` | Without config, exits 1 with informative message |
| D4 | `test_ask_success_returns_success_result` | `clank/src/processes.rs` (unit) | `AskProcess` returns `ProcessResult::success()` on exit code 0 |
| D5 | `test_no_fake_assistant_turns_in_request` | `clank-ask/src/ask_process.rs` (unit) | No `{"role":"assistant"}` in messages array when transcript present |
| D6 | `test_prompt_user_in_tool_surface` | `clank-manifest/src/lib.rs` (unit) | `subprocess_commands()` contains `prompt-user`, excludes `context` |
