---
title: "Phase 2 deviations from spec — remediation required"
date: 2026-03-06
author: agent
---

# Phase 2 Deviations from Spec

Seven deviations identified after Phase 2 implementation. All must be corrected before
Phase 2 closeout.

## Deviation 1 — `export --secret` does not export the variable (Critical)

`ExportProcess::run` records the variable name in `SecretsRegistry` but never calls
`context.shell.env.update_or_add(...)` to actually set or export the value in the shell
environment. The comment in the code incorrectly claims Brush handles this — it does not,
because our `execute_func` fully replaces Brush's `export` implementation.

## Deviation 2 — `confirm` authorization does not set P state (Significant)

The `Confirm` branch in `run_line()` directly reads from stdin inline. The current process
does not enter the `P` state. `prompt-user` is not used. The prompt format does not match
the spec. The `(a)ll` option is absent.

## Deviation 3 — `sudo` prefix breaks dispatch (Significant)

When the user types `sudo ls`, `run_line()` detects `sudo` and sets the authorization flag,
but still passes the full string `"sudo ls"` to `run_string`. Brush sees `sudo` as an
unknown command, never reaches `ls`, and the authorization flag was set for nothing.

## Deviation 4 — `prompt-user` sets P state on wrong PID (Significant)

`PromptUserProcess::run` uses `ACTIVE_SHELL_ID.with(|c| c.get())` to retrieve what it
believes is the PID, but `ACTIVE_SHELL_ID` is the shell ID, not the process ID. It calls
`process_table::set_status(shell_id, shell_id, Paused)`, which refers to a non-existent
entry. The P state is never set.

## Deviation 5 — `--secret` does not suppress echo (Minor)

`read_response` has a comment acknowledging echo suppression is not implemented. The
response is fully visible as the user types it.

## Deviation 6 — `ps aux` missing `%CPU`/`%MEM` columns (Minor)

The spec requires `ps aux` to produce standard column output with `%CPU` and `%MEM`
showing `-`. The current implementation omits those columns entirely.

## Deviation 7 — `/proc/<pid>/environ` is always empty (Minor)

The `ProcessSnapshot::environ` field is populated with `vec![]` in `shell.rs` with a
comment deferring this to "Phase 2+". This is Phase 2.
