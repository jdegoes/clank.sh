---
title: "Transcript content not passed to model in ask"
date: 2026-03-07
author: agent
---

# Transcript content not passed to model in ask

## Observed behaviour

When a user runs a command (e.g. `ls -la`) followed by `ask "what does that output tell me?"`,
the model receives no session context. It responds as if it has no knowledge of prior commands
or their output, asking the user to paste the output manually.

This is the core value proposition of clank.sh, so the failure is critical.

## Root cause (preliminary)

Not yet confirmed by investigation, but the likely location is in how the transcript is read
and passed to `run_ask` from `AskProcess` in `crates/clank/src/processes.rs`. The
`format_for_model()` call may be reading from the transcript before the command's output
has been appended, or the capture-to-transcript pipeline for registered subprocess commands
may not be writing to the shared `Arc<RwLock<Transcript>>` that `AskProcess` reads from.

## Test gaps that allowed this to go undetected

Two distinct gaps in test coverage allowed this bug to reach production undetected:

**Gap 1 — Transcript capture tests only cover Brush builtins, not registered commands.**
`crates/clank-shell/tests/transcript_capture.rs` uses `echo` (a Brush in-process builtin)
as its test command. `echo` bypasses the dual-path stdout capture mechanism entirely. No
test verifies that a registered clank command (`ls`, `cat`, `grep`, etc.) has its output
captured into the transcript via the temp-file capture path.

**Gap 2 — No end-to-end test verifies that transcript content reaches the model request.**
The `run_ask` unit tests inject `transcript_text` as a plain string parameter directly — they
do not exercise the `AskProcess` code path that reads from the shared transcript. The
`AskProcess` integration tests in `crates/clank/tests/processes.rs` verify that the AI
*response* is appended to the transcript, but never inspect the outgoing HTTP request body
to confirm that prior transcript content was included.

A single test that: (1) ran a registered command, (2) ran `ask`, and (3) asserted that the
mock HTTP request body contained the prior command's output, would have caught this
immediately.

## Acceptance criteria for the fix

1. `ask` receives the session transcript — prior commands and their outputs appear in the
   system prompt sent to the model.
2. A new crate-level integration test in `crates/clank/tests/processes.rs` (or a new file)
   verifies the end-to-end path: run a command via `ClankShell::run_line`, run `AskProcess`,
   inspect the `MockHttpClient` request body, assert the prior output appears in the system
   prompt.
3. A new test in `crates/clank-shell/tests/transcript_capture.rs` verifies that a registered
   subprocess command (`ls` or equivalent) has its output captured into the transcript — not
   just Brush builtins.
4. All existing tests continue to pass.
