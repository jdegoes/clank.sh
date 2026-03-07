---
title: "Realized design: Transcript capture for OS-fallthrough commands"
date: 2026-03-07
author: agent
plan: "dev-docs/plans/approved/transcript-capture-os-fallthrough.md"
---

# Realized design: Transcript capture for OS-fallthrough commands

## What was built

A fix in `ClankShell::run_line()` so that OS-fallthrough commands — full-path invocations
like `/bin/ls`, or any `$PATH`-resolved command not in the clank registry — now have their
stdout captured into the session transcript alongside registered commands.

## The fix

**File:** `crates/clank-shell/src/shell.rs`

The original code branched on `is_subprocess` (a manifest lookup) and placed stdout into
a capture tempfile via `params.set_fd(STDOUT_FD, ...)`. This worked for registered clank
builtins (which receive `params` via `ExecutionContext`) but not for OS-spawned processes,
which resolve stdout through Brush's persistent `shell.open_files` table rather than
through `params`.

The fix replaces the `is_subprocess` branch with an `is_internal` branch (inverted logic):

- **Shell-internal commands** (`ExecutionScope::ShellInternal`, `ParentShell`): output goes
  directly to the real terminal. Not captured. `context show` showing transcript content
  must not have that content re-recorded — doing so would create recursive self-reference
  and pollute the AI's context window.

- **All other commands** (registered subprocess commands and OS-fallthrough): **dual-path
  capture**:
  1. `params.set_fd(STDOUT_FD, capture_file.clone())` — picked up by registered builtins
     through `ExecutionContext::try_fd`.
  2. `self.inner.replace_open_files([STDIN, STDOUT=capture_file, STDERR])` — picked up by
     OS-spawned subprocesses that resolve fd 1 through `shell.persistent_open_files()`.
  
  `replace_open_files` replaces the entire `OpenFiles` struct, so all three standard fds
  must be preserved (stdin and stderr restored to real handles alongside the capture file).
  After `run_string` returns, the persistent fds are restored to the real terminal before
  the tee step.

## Why two paths are needed

Brush's `ExecutionParameters::try_fd` resolution:
```
params.open_files.try_fd(fd)   // per-invocation override
  → NotSpecified → shell.persistent_open_files().try_fd(fd)   // persistent fallback
```

For a registered builtin, `ExecutionContext` is constructed from `params` directly —
`params.set_fd` is sufficient. For an OS subprocess spawned by `compose_std_command`,
the same resolution chain applies: if `params` has the fd set, it's used; otherwise it
falls back to `shell.persistent_open_files()`. Investigation showed OS-spawned subprocesses
were not seeing the `params` override reliably, so setting both paths ensures capture
regardless of which resolution chain is exercised.

## Test coverage

**Level 2** (`crates/clank-shell/tests/transcript_os_commands.rs`), 6 tests:
- `test_os_fullpath_output_captured_in_transcript` — `/bin/echo` output in transcript
- `test_path_resolved_command_output_captured_in_transcript` — `$PATH` command captured
- `test_os_pipeline_output_captured_in_transcript` — full OS pipeline captured
- `test_registered_command_still_works_after_os_capture_change` — registered `ls` unbroken
- `test_capture_does_not_bleed_across_commands` — no cross-invocation contamination
- `test_silent_command_produces_no_output_entry` — `true` produces no output entry

**Level 3** (`tests/scenarios/shell_basics/os_fallthrough_capture.yaml`):
`/bin/echo "hello from os"` followed by `context show` — asserts the output appears in
the transcript shown by `context show`.

**Empirically verified:**
- `/bin/ls /tmp 2>/dev/null | /usr/bin/grep powerlog` — output `powerlog` appears in
  `context show`.
- `context show`, `context clear`, `context trim` — shell-internal outputs NOT re-recorded.

All tests pass. `cargo clippy` and `cargo fmt --check` pass.
