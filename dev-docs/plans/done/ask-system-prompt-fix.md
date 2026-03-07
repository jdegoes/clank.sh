---
title: "Fix ask system prompt to prevent hallucinated agentic behaviour"
date: 2026-03-07
author: agent
issue: dev-docs/issues/open/ask-system-prompt-causes-hallucinated-agentic-behaviour.md
research: []
designs: []
---

# Plan: Fix ask system prompt to prevent hallucinated agentic behaviour

## Context

The `build_system_prompt` function in `crates/clank-ask/src/ask_process.rs` currently instructs
the model that it has available tools and helps users by executing commands. This causes the
model to behave as an agent — issuing commands and hallucinating their output — rather than as
a context-aware assistant answering questions based on the session transcript.

## Task

### S1 — Rewrite `build_system_prompt` for Phase 1 context-aware assistant behaviour

Replace the current system prompt with one that:

1. Identifies the model as a shell assistant that has been given a record of the user's session.
2. Makes clear the transcript is the model's **only** source of information about the session
   — it cannot execute commands or access any information beyond what is shown.
3. Explicitly instructs the model **not** to suggest commands it would run, not to role-play
   executing them, and not to speculate about content it cannot see.
4. Removes all references to "available tools", tool-calling, and agentic execution.
5. Preserves the working directory context, which is useful factual grounding.

The rewritten prompt should be direct and unambiguous. Example:

```
You are a shell session assistant. You have been given a transcript of the user's current
shell session — the commands they have run and the output those commands produced.

Answer the user's question using only the information visible in the transcript below.
Do not suggest commands to run, do not describe steps you would take, and do not speculate
about files or system state that does not appear in the transcript.

Working directory: {cwd}
```

### S2 — Update `test_ask_build_system_prompt_*` tests to match new wording

The existing unit tests for `build_system_prompt` assert on specific prompt text. Update them
to assert the new wording, and add assertions that verify:
- No mention of "tools" or "executing" in the base prompt.
- The transcript section is still included when a non-empty transcript is provided.
- The transcript section is absent when `--fresh` is passed (transcript is `None`).

---

## Acceptance criteria

1. `ask "what can you tell me about this directory?"` after `ls -la` answers based on the `ls`
   output in the transcript without issuing further commands or hallucinating.
2. The system prompt contains no language that could be interpreted as granting tool-calling
   or command-execution capability.
3. All existing `ask` tests pass.
4. `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check` pass.

---

## Implementation notes

- This is a pure prompt change — no structural code changes are required.
- The `build_system_prompt` function signature is unchanged.
- The `--fresh` / `--no-transcript` flag behaviour is unchanged: when `fresh` is true,
  `transcript` is passed as `None` and the session transcript section is omitted.
- Do not add agentic/tool-calling language back to this prompt at any point before Phase 3
  is planned and designed. The system prompt is the contract between the phase and the model.
