---
title: "sudo ask broad authorization propagation is not implemented"
date: 2026-03-07
author: agent
---

# `sudo ask` broad authorization propagation is not implemented

## Spec

README.md § Authorization:

> "`sudo ask "..."` grants the agent broad authorization for that invocation."

This means that when a human prefixes `ask` with `sudo`, every command the agent
subsequently issues during that `ask` invocation should be treated as pre-authorized —
bypassing both `Confirm` prompts and `SudoOnly` denials for the duration of that invocation.

## Current state

`sudo ask` is syntactically accepted. The `sudo` prefix is stripped in `run_line_with_context`
and the sudo state is set for the shell. The `ask` command is dispatched normally.

However the sudo state is a per-command, one-shot flag that is cleared in `run_line_with_context`
after `run_string` returns — which happens before any agent-issued commands could arrive.
In Phase 1 this is moot because `ask` cannot issue commands. In Phase 3, when the agent
execution path exists, any commands issued by the agent will find the sudo state already cleared
and will be subject to normal enforcement.

There is also a structural problem: agent-issued commands arrive through `run_line_as_agent`,
not `run_line`. The sudo state is set in `run_line_with_context` but `run_line_as_agent` calls
the same function — so the state is set for the `ask` invocation itself, not for the commands
the agent subsequently issues. The per-invocation broad authorization grant needs a separate
mechanism: a per-shell flag indicating "this `ask` invocation was sudo-elevated", readable
from within `dispatch_builtin` when agent commands arrive.

## What is NOT the issue

- `sudo ask` being syntactically accepted is correct.
- The `ask` command itself not requiring `sudo` is correct — `ask` has an `Allow` policy.
  The elevation is about what the agent may do during the invocation, not about running `ask`.
- Agents issuing `sudo` on individual commands being denied (exit 5) is correct per spec.
  The broad grant from `sudo ask` is a different mechanism from the agent using `sudo` itself.

## Design required

A per-shell "sudo-ask active" flag, distinct from the per-command sudo state, that:

1. Is set when the human issues `sudo ask` (i.e. when `run_line_with_context` sees `sudo ask`
   as the command).
2. Is readable from `dispatch_builtin` during the agent execution path.
3. Causes `Confirm` and `SudoOnly` enforcement to be bypassed for all agent-issued commands
   during that `ask` invocation.
4. Is cleared when the `ask` invocation completes (i.e. when `run_line_with_context` returns
   after the `ask` subprocess exits).

This requires Phase 3's agent execution path to be in place first — the mechanism is only
meaningful when agents can issue commands. This issue should be addressed in Phase 3 alongside
the agent command dispatch design.

## Acceptance criteria

1. `sudo ask "..."` results in the agent being able to run `Confirm` and `SudoOnly` commands
   without prompts or denials during that invocation.
2. After the `ask` invocation completes, the broad authorization is cleared — a subsequent bare
   `ask` without `sudo` does not inherit the elevation.
3. The per-command sudo state mechanism is unchanged — this is a separate, coarser flag.
4. The existing authorization tests continue to pass.
