---
title: "Phase 2: Process model, job control, authorization, `prompt-user`, VFS, and transcript capture"
date: 2026-03-06
author: agent
updated: 2026-03-06
update_reason: >
  Revised after Phase 1 implementation revealed new constraints and clarified scope.
  Key changes: transcript capture for all commands added as a first-class deliverable;
  VFS scope narrowed to /proc/ only (MCP in Phase 3); WASM concurrency removed as a
  blocker (WASM deferred); Brush I/O redirection spike added as prerequisite; export
  --secret implementation approach noted.
---

# Phase 2: Process model, job control, authorization, `prompt-user`, VFS, and transcript capture

## Problem

The shell has no internal process table, no authorization enforcement, and no mechanism for
the AI to pause and ask the human a question. Commands that fall through to the OS bypass
transcript capture entirely. Without these, `ask` can run but cannot be safely used for
multi-step agentic tasks, and the core property of the shell — the model sees everything
the user sees — is broken.

## Capability Gap

- No process table — no PID tracking, no state transitions, no PPID.
- `ps`, `jobs`, `fg`, `bg`, `wait`, `kill` do not exist as real implementations.
- Background execution (`&`) is not wired to the process table.
- No `P` state for processes awaiting authorization or user input.
- `prompt-user` builtin does not exist.
- Authorization model is not enforced — `confirm` and `sudo-only` policies in manifests
  have no effect.
- `export --secret` is not implemented.
- `/proc/<pid>/` virtual namespace does not exist.
- Commands falling through to OS `$PATH` (e.g. `/bin/ls`) bypass transcript capture
  entirely, breaking the "model sees what you see" property.
- Core commands (`ls`, `cat`, `grep`, `stat`, `pwd`, etc.) are all stubs.
- VFS (`LayeredVfs` + `ProcHandler`) not implemented.

## Deliverables

### Process table

- Internal process table: PID (monotonically increasing, never reused), PPID, type tag,
  startup arguments, status (`R`, `S`, `T`, `Z`, `P`), start time
- Every dispatched command is registered in the process table on invocation and updated
  on completion
- `ps aux` and `ps -ef` with correct column format; `%CPU`/`%MEM` show `-`
- `jobs`, `fg`, `bg`, `wait` over synthetic processes
- Background execution (`&`) wired to process table
- `kill <pid>` for synthetic processes

### `P` state and `prompt-user`

- `P` state: process enters `P` when awaiting `prompt-user` response or authorization
  confirmation; visible in `ps` and `jobs`
- `prompt-user` builtin: Markdown on stdin rendered before question; `--choices`,
  `--confirm`, `--secret`; exit `0` on response, `130` on Ctrl-C; `--secret` responses
  never enter transcript, logs, or completion caches

### Authorization model

- `authorization-policy` enforced in `run_line()` before dispatch: `allow` (no-op),
  `confirm` (pause, call `prompt-user`, require yes/no), `sudo-only` (fail unless
  `sudo`-authorized)
- `sudo` prefix: single human authorization step; `sudo ask` grants broad authorization
  for that invocation
- `export --secret KEY=value`: variable tracked as sensitive; never echoed in `env`,
  never written to logs, never shown in `ps`, never entered into transcript. Implemented
  by intercepting/overriding Brush's `export` special builtin.

### Virtual filesystem — `/proc/` only

- `LayeredVfs` in `clank-vfs`: checks a mount table of prefix → handler, falls through
  to `std::fs` for real paths
- `ProcHandler`: serves `/proc/<pid>/cmdline`, `/proc/<pid>/status`,
  `/proc/<pid>/environ`, `/proc/clank/system-prompt` from live process table and
  manifest registry
- `/proc/` directory listing (`ls /proc/`) works
- `/proc/clank/system-prompt` computed on read from manifest registry and shell config

### Core command implementations

Implement the following as real `Process` impls using the VFS (replacing stubs). These
are the minimum set required for the transcript capture property to hold and for the
shell to be usable:

- `ls` — list directory via VFS; supports `-l`, `-a`, basic flags
- `cat` — concatenate files via VFS to stdout
- `pwd` — print working directory (already provided by Brush; verify capture works)
- `echo` — already provided by Brush; verify capture works
- `grep` — basic pattern search via VFS; supports `-r`, `-n`, `-l`
- `stat` — file metadata via VFS
- `mkdir`, `rm`, `cp`, `mv`, `touch` — basic filesystem operations via VFS (real paths
  only; `/proc/` is read-only)
- `env` — print environment variables, redacting `--secret` ones

### Transcript capture — fix OS fallthrough

- All commands dispatched through the internal process table have their stdout captured
  via the existing tempfile mechanism
- Commands that fall through to Brush's OS `$PATH` path also have their output captured
  — resolved by ensuring all commands the user is expected to use are registered in the
  dispatch table. Document that unregistered full-path invocations (e.g. `/usr/bin/vim`)
  are intentionally outside the transcript capture boundary in Phase 2; a more complete
  solution requires Phase 3+ work.
- Closes `dev-docs/issues/open/transcript-capture-os-fallthrough.md`

## Prerequisites (research spikes before plan finalisation)

**Brush I/O redirection hook** — does Brush expose a hook for intercepting `< /proc/...`
redirections in shell scripts? If not, `/proc/` paths in redirections will not work.
This must be answered before the plan is finalised. See
`dev-docs/research/virtual-filesystem-driver-options.md` for context.

**`export` override mechanism** — how do we add `--secret` flag support to Brush's
native `export` special builtin without forking Brush? Options: register our own
`export` builtin that shadows Brush's, or intercept via Brush's declaration builtin API.

## Open Questions No Longer Blocking

**In-WASM concurrency** (previously Gap 10) — WASM target deferred; not a Phase 2
concern. Native concurrency uses tokio async tasks.

## Out of Scope

MCP, Golem, `grease`, tab completion, `ask repl`, virtual `/mnt/mcp/` namespace (Phase 3),
`/bin/` virtual namespace (Phase 5), `Ctrl-Z` / SIGTSTP (native TTY, Phase 5).
`/proc/` I/O redirection support in scripts — deferred if Brush does not expose the hook
(noted as a known limitation).
