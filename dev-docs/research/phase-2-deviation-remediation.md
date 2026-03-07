---
title: "Phase 2 Deviation Remediation — Research"
date: 2026-03-06
author: agent
---

# Phase 2 Deviation Remediation Research

## Deviation 1: `export --secret` — how to mutate shell env from dispatch

`ExecutionContext` (brush-core/src/commands.rs:28) has `pub shell: &'a mut Shell` — the
shell is mutable from within `dispatch_builtin`. The env API is:

```rust
context.shell.env.update_or_add(
    name,
    ShellValueLiteral::Scalar(value),
    |var| { var.export(); Ok(()) },
    EnvironmentLookup::Anywhere,
    EnvironmentScope::Global,
)
```

However: `dispatch_builtin` is an `async fn` receiving `ExecutionContext<'_>` by value.
The `ExecutionContext` is consumed by `process.run(ctx)`. We need the shell reference
before it is moved into `ProcessContext`.

**Solution:** Handle `export` specially in `dispatch_builtin` before calling
`process.run()`. When `cmd_name == "export"`, extract `--secret`, perform the env
mutation via `context.shell.env`, then call through to the process for secret tagging.
The declaration builtin machinery in Brush (which processes `KEY=value` args) runs before
`execute_func` is called, so by the time we reach `dispatch_builtin`, Brush has already
parsed the assignment args — we just need to handle `--secret` and mark the variable.

Concretely: in `dispatch_builtin`, before building `ProcessContext`, check if `cmd_name
== "export"` and `args` contains `--secret`. If so, iterate the assignment args and call
`context.shell.env.update_or_add(name, value, |v| { v.export(); Ok(()) }, ...)` for each.
The `ExportProcess` then only needs to record the secret name — the env mutation is done
in `dispatch_builtin` where we still have the `ExecutionContext`.

Wait — there's a subtlety: Brush's declaration builtin machinery runs *inside*
`execute_func`, not before it. The raw `args: Vec<CommandArg>` that arrive at our
`dispatch_builtin` still contain the unparsed assignment args. Brush's own `export`
implementation (via `DeclarationCommand`) processes them. Since we replaced that with our
stub `ExportProcess`, we get the raw args and must process them ourselves.

**Confirmed approach:**
1. In `dispatch_builtin`, when `cmd_name == "export"`: parse args, perform env mutation
   via `context.shell.env`, call `ExportProcess::run` only for the `--secret` side effect.
2. `ExportProcess::run` records secret names; env mutation happens in `dispatch_builtin`.

Registration note: set `declaration_builtin: true` in the `export` Registration so Brush
routes assignment-style args through the declaration machinery (which populates the
`CommandArg::Assignment` variants rather than treating `KEY=value` as a string arg).

## Deviation 2: `confirm` P state — correct approach

The P state must be set on the process being dispatched. The dispatch flow is:
`run_line() → run_string() → dispatch_builtin()`. The PID is assigned in `dispatch_builtin`
just before `process.run()`. To set P state before blocking on the confirmation:

1. Assign PID in `dispatch_builtin` (already done).
2. For `Confirm`-policy commands: set P state on that PID before presenting the prompt.
3. Use `prompt-user` logic inline (or extract a shared function) — the confirmation
   prompt is a special case of `prompt-user --confirm`.
4. Reset to R state after confirmation; proceed or abort.

This means the P-state-setting moves from `run_line()` authorization check into
`dispatch_builtin`, where the PID is known.

## Deviation 3: `sudo` prefix — strip from line before dispatch

`run_string` must receive `"ls"` not `"sudo ls"`. Fix: when `sudo` is detected as a prefix
in `run_line()`, build a modified line string with `"sudo "` stripped before passing to
`run_string`. The SUDO_STATE is set; the stripped line is dispatched.

## Deviation 4: PID threading — how to make it available to Process impls

Three options:

**A. Thread PID through `ProcessContext`.** Add `pid: u64` to `ProcessContext`. Set it in
`dispatch_builtin` after calling `process_table::spawn(...)`. The PID is then available
to every `Process::run()` via `ctx.pid`.

**B. Thread-local PID.** Add a `ACTIVE_PID` thread-local alongside `ACTIVE_SHELL_ID`.
Set it in `dispatch_builtin` after `spawn`. Available to any code on the same thread.

**C. Process self-registration.** Let `Process::run` call `process_table::get_by_argv()`
to find its own entry. Fragile — multiple processes can have the same argv.

**Recommendation: A.** `ProcessContext` already carries contextual data; adding `pid`
is clean, testable, and avoids another global.

## Deviation 5: Secret echo suppression

`rpassword::read_password()` reads from the TTY with echo disabled. API:
```rust
let password = rpassword::read_password().unwrap();
```
No platform-specific code needed; crate handles Unix/macOS/Windows.

## Deviation 6: `ps aux` column format

Spec: `%CPU` and `%MEM` columns present, showing `-`. Add them to the format string.

Standard `ps aux` columns: `USER PID %CPU %MEM VSZ RSS TTY STAT START TIME COMMAND`
Standard `ps -ef` columns: `UID PID PPID C STIME TTY TIME CMD`

For our implementation: show `-` for all columns that are not meaningful
(`%CPU`, `%MEM`, `VSZ`, `RSS`, `TTY`, `START`, `TIME`). Show the real values for
`PID`, `PPID`, `STAT`, `COMMAND`.

## Deviation 7: `/proc/<pid>/environ` population

The shell's current environment is accessible via `context.shell.env.iter_exported()`.
But `ProcessSnapshot` is built in `shell.rs` before dispatch, not inside
`dispatch_builtin` where `ExecutionContext` is available.

**Fix:** Build environ from `std::env::vars()` (the process's own env) when creating
the snapshot, filtering out secret variable names via `SecretsRegistry`. This gives a
reasonable approximation for Phase 2. Full per-subprocess environment tracking is a
Phase 3+ concern.
