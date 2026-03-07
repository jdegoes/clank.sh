---
title: "ask system prompt fix — realized design"
date: 2026-03-07
author: agent
---

# Realized Design: ask system prompt fix

## Root cause confirmed

`build_system_prompt` in `ask_process.rs` instructed the model that it had "available tools:
every subprocess-scoped command on $PATH" and helped users "by executing commands and
interpreting their output." This caused the model to behave as an agent with tool-calling
capability. Since `ask` in Phase 1 has no feedback loop for command execution, the model
fabricated plausible-looking but entirely fictional output for the commands it "ran".

## What was built

**`build_system_prompt` rewritten.** The new prompt:

```
You are a shell session assistant. You have been given a transcript of the
user's current shell session — the commands they have run and the output those
commands produced.

Answer the user's question using only the information visible in the session
transcript below. Do not suggest commands to run, do not describe steps you
would take, and do not speculate about files or system state that does not
appear in the transcript.

Working directory: {cwd}
```

Key changes:
- Positions the model as a reader of existing context, not an actor
- Explicitly prohibits suggesting commands or speculating beyond the transcript
- Removes all references to "available tools", tool-calling, and agentic execution

## Test changes

Two existing system prompt tests updated to assert the corrected wording. Both now also assert
the *absence* of `Available tools` and `executing commands` — acting as regression guards
against agentic language being reintroduced before Phase 3 is designed.

## Phase 3 note

The system prompt will need to be revisited in Phase 3 when the agent execution path is
introduced. At that point, `run_line_as_agent` will be the entry point and the system prompt
sent to the agent will need to describe its available tools accurately. The Phase 1 prompt
must not be modified to add tool-calling language until that design is approved.
