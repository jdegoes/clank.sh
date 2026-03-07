---
title: "Disable Confirm/SudoOnly enforcement for user-typed commands in Phase 1"
date: 2026-03-07
author: agent
---

# Disable Confirm/SudoOnly enforcement for user-typed commands in Phase 1

## Context

See dev-docs/issues/open/authorization-context-user-vs-agent.md for the full architectural
issue. This issue tracks the short-term fix only.

## Problem

In Phase 1, all commands are user-typed. The agentic execution path does not yet exist. However,
`Confirm` and `SudoOnly` authorization policies are being enforced on user-typed commands,
requiring confirmation for `mkdir`, `touch`, `cp`, `mv`, and sudo for `rm` and `kill`. This
breaks normal interactive shell use and blocks the tutorial.

## Fix

Until the two-context authorization model (user vs. agent) is designed and implemented, treat
every command in the interactive REPL as user-typed — i.e. bypass `Confirm` and `SudoOnly`
enforcement for all commands in Phase 1.

The manifest policy values (`Confirm`, `SudoOnly`) must be preserved as-is: they are the
correct policies for the future agent context and must be in place for Phase 3. Only the
enforcement point changes — the check in `run_line` / `dispatch_builtin` should be
short-circuited in Phase 1.

The cleanest way to achieve this without adding a permanent flag is to check whether the
active shell's execution originated from the REPL (user context) vs. a future agent dispatch
path. For now, since no agent path exists, the enforcement can simply be disabled in
`dispatch_builtin` and `run_line` with a clear `// TODO(Phase 3): re-enable when agent
execution path is introduced` comment.

## Spec constraint: agents cannot use sudo

Per README.md § Authorization:

> "Agents cannot use sudo. An agent that needs elevation must pause and surface a
> confirmation request."
> "sudo ask '...' grants the agent broad authorization for that invocation."

`sudo` is a human gesture only. In agent context, a `sudo` prefix on a command must be
denied (exit 5), not granted. The only elevation path for an agent is:

1. The human types `sudo ask "..."` — granting broad authorisation to that entire `ask`
   invocation (a separate mechanism, not yet implemented in Phase 1).
2. The agent uses `prompt-user` to pause and ask the human for confirmation.

This means `SudoOnly` commands are **always** denied in agent context — there is no
sudo-grant path available to the agent itself.

## Also fixes

- `rm` and `kill` no longer require `sudo` for direct user use — consistent with normal shell
  behaviour.

## Acceptance criteria

1. `mkdir demo` creates the directory without prompting.
2. `touch file.txt` creates the file without prompting.
3. `rm file.txt` removes the file without requiring `sudo`.
4. `sudo rm file.txt` typed by the user works (one-shot human elevation).
5. An agent issuing `sudo rm` is denied (exit 5) — agents cannot use sudo.
6. An agent issuing bare `rm` is denied (exit 5) — SudoOnly is enforced in agent context.
7. The manifest policy values for these commands are unchanged.
8. A comment at each enforcement point cites the spec and both issues.
