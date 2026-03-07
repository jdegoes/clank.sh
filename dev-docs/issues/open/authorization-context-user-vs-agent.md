---
title: "Authorization policies must distinguish user-typed commands from agent-issued commands"
date: 2026-03-07
author: agent
---

# Authorization policies must distinguish user-typed commands from agent-issued commands

## The problem

The authorization system currently applies `Confirm` and `SudoOnly` policies uniformly to all
commands, regardless of whether the command was typed directly by the user or issued
autonomously by a model. This conflates two fundamentally different execution contexts:

**User-typed commands:** The user has already made the decision by typing the command. Requiring
them to confirm `mkdir demo` or `touch file.txt` is paternalistic — the shell is second-guessing
the human who is in full control. It breaks normal shell workflows and makes clank.sh more
cumbersome than a plain bash session.

**Agent-issued commands:** When a model autonomously issues commands on the user's behalf (Phase
3 MCP, Phase 4 Golem), confirmation and sudo policies are the correct safety mechanism. The model
may have misunderstood the task, hallucinated a path, or be about to do something the user did
not intend. Speed bumps are valuable here.

## Current state

In Phase 1, `ask` is a single-turn assistant with no command-execution capability. The agentic
execution path does not yet exist. However, the authorization policies designed for that future
path are already being enforced on user-typed commands, making basic operations like `mkdir`,
`touch`, `cp`, and `mv` require confirmation.

## Desired behaviour

| Command source | `Allow` | `Confirm` | `SudoOnly` |
|---|---|---|---|
| User-typed | Execute immediately | Execute immediately | Execute immediately (user is already authorizing by typing) |
| Agent-issued | Execute immediately | Pause and confirm with user | Require explicit `sudo` prefix or equivalent grant |

## Design required

This requires a two-context authorization model:

1. **Execution context** — a flag or enum on the dispatch path indicating whether the current
   command originated from user input or from an agent. This should be threaded through
   `ProcessContext` or the dispatch layer.

2. **Policy enforcement** — the authorization check in `run_line` / `dispatch_builtin` gates
   `Confirm` and `SudoOnly` enforcement on the execution context. User-typed commands bypass
   these gates entirely.

3. **Agent entrypoint** — when Phase 3 introduces model-issued command execution, it must set
   the execution context to `Agent` so the policies apply correctly.

## Short-term fix (separate issue)

Until this is designed and implemented, the immediate tutorial breakage should be addressed by
disabling the `Confirm` and `SudoOnly` enforcement for all commands in Phase 1 — i.e. treating
every command as user-typed until the agent execution path exists. This is a temporary measure
and must be clearly marked as such.

See: dev-docs/issues/open/authorization-bypass-for-user-context-phase1.md
