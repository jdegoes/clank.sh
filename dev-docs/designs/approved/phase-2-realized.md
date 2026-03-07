---
title: "Phase 2: Process Model — Realized Design"
date: 2026-03-06
author: agent
realized_design: true
supersedes: "dev-docs/plans/approved/phase-2-process-model.md"
---

# Phase 2: Process Model — Realized Design

## Overview

This document records the Phase 2 process model as actually implemented, including the
seven deviation remediations applied after the initial implementation. It supersedes the
Phase 2 plan as the authoritative reference for future work. The plan remains as permanent
historical record.

Deviations from the plan that were corrected during remediation are noted inline.

---

## Process Table (`clank-shell/src/process_table.rs`)

A global `LazyLock<RwLock<HashMap<(shell_id, pid), ProcessEntry>>>` tracks all synthetic
processes.

### `ProcessEntry`

| Field | Type | Description |
|---|---|---|
| `pid` | `u64` | Monotonically increasing per-shell PID |
| `ppid` | `u64` | Parent PID (0 for top-level dispatch) |
| `shell_id` | `u64` | The shell instance this process belongs to |
| `process_type` | `ProcessType` | `ParentShell`, `ShellInternal`, or `Subprocess` |
| `argv` | `Vec<String>` | Full argument vector including `argv[0]` |
| `status` | `ProcessStatus` | Current state (see below) |
| `start_time` | `SystemTime` | Wall clock at spawn time |
| `abort_handle` | `Option<AbortHandle>` | For backgrounded tasks; `None` for foreground |

### `ProcessStatus` states

| Variant | `state_char` | Meaning |
|---|---|---|
| `Running` | `R` | Active / executing |
| `Sleeping` | `S` | Waiting on remote work |
| `Suspended` | `T` | Ctrl-Z suspended (Phase 5+) |
| `Zombie { exit_code }` | `Z` | Completed, not yet reaped |
| `Paused` | `P` | Awaiting user input (`prompt-user` or Confirm) |

### API

```rust
process_table::spawn(shell_id, ppid, argv, process_type) -> u64  // returns pid
process_table::complete(shell_id, pid, exit_code)
process_table::reap(shell_id, pid)
process_table::set_status(shell_id, pid, status)
process_table::snapshot(shell_id) -> Vec<ProcessEntry>
process_table::get(shell_id, pid) -> Option<ProcessEntry>
```

---

## `ProcessContext` (`clank-shell/src/process.rs`)

```rust
pub struct ProcessContext {
    pub argv: Vec<String>,
    pub env: HashMap<String, String>,
    pub io: ProcessIo,
    pub pid: u64,  // Added in Dev 4 remediation
}
```

`pid` is set by `dispatch_builtin` after `process_table::spawn()`. Every `Process::run`
implementation receives the correct process table PID via `ctx.pid`.

---

## `dispatch_builtin` (`clank-shell/src/builtins.rs`)

The bare `fn` pointer registered with Brush as each command's `execute_func`. It is the
central dispatch hub for all clank commands.

### Execution flow

1. **Export special-case (Dev 1):** When `cmd_name == "export"`, iterate
   `CommandArg::Assignment` args and call `ctx.shell.env.update_or_add(...)` to perform
   real shell-environment mutation before the `ExportProcess` is invoked. The `--secret`
   flag is detected and the variable name is registered in `SecretsRegistry`.

2. **Argv collection:** All args are converted to strings (`CommandArg::Assignment` via
   `.to_string()`).

3. **Process type lookup:** The manifest registry is consulted to determine
   `ProcessType` (ParentShell / ShellInternal / Subprocess).

4. **Spawn:** `process_table::spawn(...)` registers the process and returns the PID.

5. **Confirm authorization (Dev 2):** When the command's manifest policy is `Confirm`
   and `SUDO_STATE` is not set, the process enters `Paused` state, presents the prompt
   `"<cmd> requires confirmation. (y)es, (n)o: "`, waits for user input, then restores
   `Running` state. If the user does not confirm, the process is completed with exit 1 and
   the function returns early.

6. **SUDO_STATE clear:** `SUDO_STATE` is cleared after each command (one-shot grant).

7. **Process dispatch:** The process is looked up in the dispatch table and
   `process.run(ProcessContext { argv, env: {}, io, pid })` is called.

8. **Exit code mapping:** Exit code 0 maps to `ExecutionExitCode::Success` (required for
   `&&` short-circuit logic). Non-zero exit codes use `ExecutionExitCode::Custom(n)` to
   preserve the numeric value for `run_line` to return to callers.

### `export` registration

`export` is registered with `declaration_builtin: true` so Brush passes assignment-style
args as `CommandArg::Assignment` (with expanded values) rather than plain strings. `export`
is also included in the commands list so our Registration overrides Brush's built-in
`export` (which uses clap and rejects the `--secret` flag).

---

## `run_line` (`clank-shell/src/shell.rs`)

### Authorization and line stripping

1. Detect `sudo` prefix: if present, set `SUDO_STATE = true` and compute
   `stripped_line` (original line with `"sudo "` removed).
2. For `SudoOnly` commands: deny immediately if not sudo (exit 5), clear SUDO_STATE,
   return.
3. For `Confirm` commands: **no action here** — handled in `dispatch_builtin` where the
   PID is known.
4. The `effective_line` (stripped of `sudo`) is what gets passed to `run_string`.
   The transcript always records the original line (with `sudo`).

### /proc/ snapshot

At the start of each `run_line` call, the process table is snapshotted into
`proc_snapshot`. Each `ProcessSnapshot` now includes `environ: Vec<(String, String)>`
populated from `std::env::vars()`, filtered to exclude keys present in
`SecretsRegistry::snapshot()`. This fixes the always-empty `environ` deviation.

---

## `ExportProcess` (`clank-shell/src/commands/export.rs`)

Handles only the `--secret` side effect: for each declaration arg with the `--secret`
flag, registers the variable name in `SecretsRegistry`. The actual shell environment
mutation is now performed by `dispatch_builtin` before `ExportProcess::run` is called.

---

## `PromptUserProcess` (`clank-shell/src/commands/prompt_user.rs`)

P-state transitions now use `ctx.pid` (from the `ProcessContext` field added in Dev 4)
instead of the incorrect `ACTIVE_SHELL_ID` (which is the shell ID, not a process PID).

### Secret echo suppression (Dev 5)

When `--secret` is passed, `read_response` calls `rpassword::read_password()` instead
of `stdin.lock().read_line()`. This reads from the TTY with echo disabled.

---

## `PsProcess` (`clank-shell/src/commands/ps.rs`)

### Column formats

**`ps aux` / `ps ax`** — full standard column format:
```
USER       PID  %CPU %MEM   VSZ   RSS TTY  STAT START TIME COMMAND
```
Non-meaningful columns (`%CPU`, `%MEM`, `VSZ`, `RSS`, `TTY`, `START`, `TIME`) display `-`.

**`ps -ef`** — standard extended format:
```
UID        PID  PPID C STIME TTY  TIME CMD
```
Non-meaningful columns display `-`.

**`ps`** (bare) — compact format:
```
  PID STAT COMMAND
```

---

## SecretsRegistry (`clank-shell/src/secrets.rs`)

Unchanged from Phase 1. Global `LazyLock<RwLock<HashSet<String>>>` keyed by variable name.

### Interaction with `/proc/<pid>/environ`

When populating `ProcessSnapshot::environ`, keys present in `SecretsRegistry::snapshot()`
are filtered out. Secret values are never exposed through the VFS.

---

## VFS and `/proc/` (`clank-vfs`)

The `proc_handler::ProcHandler` is unchanged. `ProcessSnapshot::environ` is now
non-empty: populated from `std::env::vars()` with secrets filtered.

`/proc/<pid>/environ` returns NUL-separated `KEY=value` pairs for all non-secret
environment variables of the process.

---

## Known limitations

| Area | Limitation | Resolution |
|---|---|---|
| `prompt-user` P state | Confirm prompt P state is set in `dispatch_builtin`, not `run_line`. The PID is correct, but no test can verify the state transition without mocking stdin. | Verified by code inspection |
| `prompt-user` `--secret` tests | `rpassword::read_password()` reads from the real TTY; cannot be tested in CI without a PTY harness. | Code path verified; manual testing required |
| `/proc/<pid>/environ` timing | The snapshot is taken at the start of `run_line`, before commands are dispatched. A process's own PID is never in the snapshot at the time its command reads `/proc/<self>/environ`. | Reasonable for Phase 2; per-subprocess env tracking is Phase 3+ |
| `dispatch_builtin` exit code | `Custom(0)` was previously used for all exit codes, causing `&&` to short-circuit after every dispatched command. Fixed: 0 maps to `Success`. | Fixed in Dev remediation |

---

## Acceptance tests

All acceptance tests from `dev-docs/plans/approved/phase-2-deviation-remediation.md` pass:

| Test | Location | Status |
|---|---|---|
| `test_export_sets_env_variable` | `clank-shell/tests/context.rs` | PASS |
| `test_export_secret_registers_in_secrets` | `clank-shell/tests/context.rs` | PASS |
| `test_sudo_strips_prefix_from_dispatch` | `clank-shell/tests/context.rs` | PASS |
| `test_ps_aux_has_cpu_mem_columns` | `clank-shell/tests/context.rs` | PASS |
| `test_ps_aux_header_has_cpu_mem_columns` | `clank-shell/src/commands/ps.rs` | PASS |
| `test_ps_ef_header_has_standard_columns` | `clank-shell/src/commands/ps.rs` | PASS |
| `test_proc_environ_not_empty` | `clank-shell/tests/transcript_capture.rs` | PASS |
| All pre-existing tests | workspace | PASS |
