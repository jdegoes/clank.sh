---
title: "Phase 1 deviations from spec — remediation required"
date: 2026-03-06
author: agent
---

# Phase 1 Deviations from Spec

Six deviations from the spec were identified after Phase 1 implementation. Each must be
remediated before Phase 1 closeout.

## Deviation 1 — Output not captured in transcript (High)

**Spec:** "The shell maintains a sliding-window transcript of everything rendered to the
terminal: every command typed, **every output produced**, every AI response."

**What we built:** `run_line` records commands only. Command output is not captured. The AI
cannot see what any command produced.

**Root cause:** Capturing Brush's stdout via a concurrent pipe reader deadlocked. The attempt
was abandoned and output capture silently dropped.

## Deviation 2 — Piped stdin to `ask` silently discarded (High)

**Spec:** `cat error.log | ask "summarize this"` is an explicit first-class usage. "When
content is piped to `ask`, it arrives on `ask`'s stdin as supplementary input."

**What we built:** `processes.rs` unconditionally sets `piped = Vec::new()`. No error is
emitted. A user piping input to `ask` receives a response that silently ignores that input.

**Root cause:** Unable to distinguish `PipeReader` stdin from terminal stdin at dispatch time.

## Deviation 3 — `context summarize` prints entry count, not a summary (Medium)

**Spec:** "`context summarize` — print a summary of the current transcript to stdout."
The manual compaction idiom `SUMMARY=$(context summarize) && context clear && echo "$SUMMARY"`
is explicitly demonstrated.

**What we built:** `context summarize` prints `"Transcript has N entries."` — useless as a
summary and useless in the compaction idiom.

**Root cause:** Deferred without adequate justification. The provider machinery needed to call
the model was available by Phase 1 completion.

## Deviation 4 — `ProcessResult::failure` used for all exit codes (Low)

**What we built:** `AskProcess` and `ModelProcess` return `ProcessResult::failure(exit_code)`
even when `exit_code == 0`. The name is misleading and could confuse future implementers.

## Deviation 5 — Fake assistant turn injected into transcript messages (Medium)

**Spec:** The model receives the transcript window as context.

**What we built:** The transcript is sent as a `User` message, then followed by a fabricated
`Assistant` message `"I have read the session transcript."` This fake dialogue turn is not in
the spec, inflates token usage, and can confuse models.

## Deviation 6 — `prompt-user` excluded from `ask` tool surface (Medium)

**Spec (line 450):** "The exception is `prompt-user`: although `shell-internal`, it is
explicitly exposed to the model as a tool because it is the mechanism by which the model
communicates back to the human during a task."

**What we built:** `ManifestRegistry::subprocess_commands()` filters by
`ExecutionScope::Subprocess` only. `prompt-user` is registered as `ShellInternal` and is
therefore excluded from the tool surface sent to the model.
