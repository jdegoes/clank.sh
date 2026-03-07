---
title: "Pre-review code quality remediation — realized design"
date: 2026-03-07
author: agent
---

# Realized Design: Pre-review code quality remediation

## What was built

A systematic sweep of the codebase identified and fixed 15 issues across three groups.

### Group A — Architectural

**A1. `context summarize` now uses `AnthropicProvider`.**
`context_process.rs` previously contained a hand-rolled HTTP call duplicating
`AnthropicProvider`. It now constructs an `AnthropicProvider` from the config's api_key and
the injected `Arc<dyn HttpClient>`, routing through the provider abstraction correctly. The
`ContextProcess::with_config()` constructor was added for test injection.

**A2. `model remove`/`model info` exit code corrected to 2.**
These unimplemented stubs returned exit 1 (general error). Per the exit code table, exit 2
is "invalid usage / bad arguments" — which calling an unimplemented subcommand is. Corrected,
and a companion test for `model info` was added alongside renaming `test_model_remove_stub_exits_1`.

### Group B — Convention violations

**B1.** Eliminated `.unwrap()` on `base_url` in `run_model_list` by extracting the value
before the match rather than unwrapping inside a guard-checked arm.

**B2.** Renamed `test_context_summarize_parse_failure_exits_1` to
`test_context_summarize_wrong_shape_exits_1_with_error`.

**B3.** `shell.rs:377` — temp file read failure now emits `tracing::warn!` rather than
silently discarding the error with `unwrap_or_default()`.

**B4.** `scenario.rs` `_desc` field — `#[allow(dead_code)]` removed; field renamed to `_desc`
using the Rust convention for intentionally unused fields.

**B5.** Blocking `stdin().lock().read_line()` inside the async `Confirm` dispatch wrapped in
`tokio::task::spawn_blocking`, consistent with the REPL loop pattern.

### Group C — Rust idioms

**C1.** Six `push_str(&format!(...))` sites replaced with `write!()`/`writeln!()` via
`std::fmt::Write`, eliminating temporary String allocations.

**C2.** Double `argv.clone()` in `PromptUserProcess` eliminated — `ctx.argv` is now borrowed
directly without cloning.

**C3.** `ModelOutput { stdout, stderr, exit_code }` and `AskOutput { stdout, stderr, exit_code }`
named structs introduced for the return types of `run_model` and `run_ask`. All ~50 call-site
destructurings updated. `ModelOutput::ok()` and `ModelOutput::err()` convenience constructors
reduce boilerplate.

**C4.** All 9 command process structs given `pub fn new(...)` constructors with private fields.
`shell.rs` construction sites updated to use constructors. `AskProcess` in `processes.rs`
similarly updated.

**C5.** `buf.clone()` in the REPL multi-line input probe replaced with `buf.as_str()`, which
`parse_string`'s `S: Into<String>` bound accepts without allocation.

## Key decisions

- `AskOutput.stderr` is `Vec<u8>` not `String` — preserving binary-safe error output.
- `ModelOutput` and `AskOutput` are separate structs because their stderr types differ.
- Wire format tests for OpenRouter/OpenAI-compat now assert the absence of a top-level `system`
  field (was being asserted as present — validating the bug not the spec).

## Test changes

- `test_context_summarize_wrong_shape_exits_1_with_error` — updated to assert exit 1 (correct)
  not exit 0 (was testing wrong/old graceful degradation behaviour)
- `test_model_info_stub_exits_2` added
- Two new wire format spec assertions in OpenRouter and OpenAI-compat provider tests
