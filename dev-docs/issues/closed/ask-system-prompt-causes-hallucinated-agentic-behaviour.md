---
title: "ask system prompt causes hallucinated agentic behaviour"
date: 2026-03-07
author: agent
---

# ask system prompt causes hallucinated agentic behaviour

## Observed behaviour

When a user runs `ask "what can you tell me about this directory?"` after `ls -la`, the model
responds by issuing `cat README.md`, `ls crates/`, etc. as if it were an agent with tool-calling
capability. Because `ask` is a single-turn subprocess — not an agent — the commands are not
actually executed. The model then fabricates plausible-looking but entirely fictional output for
each invented command, producing a confident and detailed response that is completely wrong.

## Root cause

The system prompt constructed by `build_system_prompt` in
`crates/clank-ask/src/ask_process.rs:348` tells the model:

> "You are clank, an AI-native shell running on Linux. You help the user with tasks by
> **executing commands and interpreting their output.**"
> …
> "Available tools: every subprocess-scoped command on $PATH"

This is an instruction to behave as an agent with tool-calling capability. The model correctly
follows the instruction — it just has no actual mechanism to execute commands or receive their
output. Since there is no feedback loop, it hallucinates the output of the commands it "runs".

## What `ask` should be at this phase

In Phase 1, `ask` is a single-turn context-aware assistant. It receives:
- The session transcript (commands the user has already run, and their real output)
- The user's question

It should answer based solely on what it can already see in the transcript. It cannot and
should not execute further commands. That capability belongs to Phase 3 (MCP tool calls) and
Phase 4 (Golem agents).

The system prompt must be rewritten to:
1. Tell the model it is answering a question about the current shell session.
2. Tell it the session transcript is provided as its only source of information.
3. Explicitly tell it NOT to issue commands or describe steps it would take — it should
   answer based on what it already knows from the transcript.
4. Remove all references to "available tools", tool execution, and agentic behaviour.

## Acceptance criteria

1. `ask "what can you tell me about this directory?"` after `ls -la` produces a direct answer
   based on the `ls` output already in the transcript — without the model issuing any further
   commands or fabricating output.
2. The system prompt contains no instructions that could be interpreted as granting tool-calling
   or command-execution capability.
3. The existing `ask` tests continue to pass, including the new end-to-end transcript test.
