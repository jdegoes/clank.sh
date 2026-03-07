---
title: "Authorization user vs. agent context (Phase 1) — realized design"
date: 2026-03-07
author: agent
---

# Realized Design: Authorization — user vs. agent context (Phase 1)

## What was built

### `ExecutionContext` enum

```rust
pub enum ExecutionContext { User, Agent }
```

Defined in `builtins.rs`, exported from `lib.rs`. Carried via `ACTIVE_EXECUTION_CONTEXT`
thread-local alongside the existing `ACTIVE_SHELL_ID`. Defaults to `User`.

### Two public entry points

- `ClankShell::run_line(&mut self, line: &str)` — sets `User` context. All interactive REPL
  input goes through here. No authorization enforcement.
- `ClankShell::run_line_as_agent(&mut self, line: &str)` — sets `Agent` context. Phase 3
  agent command dispatch will use this. Full authorization enforcement.

Both delegate to the private `run_line_with_context(line, context)`.

### Enforcement points

Two `TODO(Phase 3)` comments mark where enforcement is gated:

1. **`shell.rs` — `SudoOnly` early deny**: `if context == Agent { ... }`. In agent context,
   a `sudo` prefix is also an immediate deny (exit 5) — agents cannot use `sudo`; that is a
   human gesture only.

2. **`builtins.rs` — `Confirm` prompt**: `if execution_context == Agent { ... }`. The prompt
   and `spawn_blocking` readline are inside this guard.

### Agent sudo semantics (per spec)

Per `README.md § Authorization`: "Agents cannot use sudo." In agent context:
- A `sudo` prefix on any command → immediate exit 5
- `SudoOnly` commands → always exit 5 (no elevation path for agents)
- Elevation for agents comes only from `sudo ask` at the human level (D13, Phase 3)

### Tests — both sides covered

**Side A (user context):**
- `test_user_confirm_command_executes_without_prompt` — `mkdir` runs freely
- `test_user_sudo_only_command_executes_without_sudo` — `rm` exits 1 not 5
- `test_user_sudo_prefix_still_strips_and_executes` — `sudo rm` works normally

**Side B (agent context):**
- `test_agent_sudo_only_denied_without_sudo` — `rm` exits 5
- `test_agent_sudo_prefix_is_denied` — `sudo rm` exits 5 (agents cannot use sudo)
- `test_agent_confirm_command_aborts_without_grant` — `mkdir` with EOF stdin exits 1
- `test_agent_sudo_state_cleared_after_command` — one-shot sudo state clears per command

## What remains (D13)

`sudo ask` broad authorization propagation is not implemented. The per-command sudo state is
set when the human types `sudo ask`, but is cleared before any agent-issued commands could
arrive. A per-invocation "sudo-ask active" flag is needed — see
`dev-docs/issues/open/sudo-ask-broad-authorization.md`. Must be addressed in Phase 3.
