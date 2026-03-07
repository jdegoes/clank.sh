---
title: "Brush Command Dispatch and Intercept Mechanisms"
date: 2026-03-06
author: agent
---

# Brush Command Dispatch and Intercept Mechanisms

## Motivation

clank.sh replaces Brush's Unix process spawning layer with its own internal `Process` trait. This
research investigates exactly how `brush-core` dispatches external commands and what hook points
are available to an embedder.

## Source Files Reviewed

- `brush-core/src/commands.rs` — primary dispatch logic
- `brush-core/src/processes.rs` — child process management
- `brush-core/src/sys/tokio_process.rs` — the actual OS spawn call
- `brush-core/src/sys/unix/commands.rs` — Unix-specific pre-exec hooks
- `brush-core/src/builtins.rs` — builtin trait and `Registration` type
- `brush-core/src/extensions.rs` — `ShellExtensions` trait
- `brush-core/src/interfaces.rs` — caller-implementable traits
- `brush-core/src/shell/builder.rs` — `ShellBuilder` API
- `brush-core/src/shell/builtin_registry.rs` — runtime builtin registration

## Dispatch Chain

`SimpleCommand::execute()` in `commands.rs` resolves commands in strict priority order:

1. POSIX special builtins (if `posix_mode`)
2. Shell functions
3. Regular builtins
4. External command: resolve path via `$PATH` cache → `execute_via_external(path)`

`execute_external_command()` calls `compose_std_command()` to build a `std::process::Command`,
then calls `sys::process::spawn(cmd)`, which is:

```rust
// brush-core/src/sys/tokio_process.rs
pub(crate) fn spawn(command: std::process::Command) -> std::io::Result<Child> {
    let mut command = tokio::process::Command::from(command);
    command.spawn()
}
```

This is the only place external processes are spawned. It is `pub(crate)` — not accessible to
embedders.

## Caller-Implementable Traits in brush-core

| Trait | Location | Purpose |
|---|---|---|
| `ShellExtensions` | `extensions.rs` | Compile-time generic on `Shell<SE>`; currently only carries `ErrorFormatter` |
| `ErrorFormatter` | `extensions.rs` | Formats error messages |
| `KeyBindings` | `interfaces/keybindings.rs` | Readline-style key binding management |
| `builtins::Command` | `builtins.rs` | Implements a clap-backed builtin |
| `builtins::SimpleCommand` | `builtins.rs` | Implements a simpler builtin |
| `builtins::DeclarationCommand` | `builtins.rs` | Implements a declaration builtin |

**None of these traits intercept the path between "not a builtin" and `sys::process::spawn()`.**

The `interfaces` module exports only `KeyBindings` and its supporting types. There is no process
spawner interface, no command resolver interface, and no "command not found" hook.

`ShellExtensions` has an associated `ErrorFormatter` type and stub infrastructure for future
extensibility (`PlaceholderBehavior`, `DefaultPlaceholder`) but no `ProcessSpawner` associated
type is wired into the dispatch path.

## What IS Possible Without Forking

**Registering builtins by name intercepts before external resolution.** Because builtins are
checked before `$PATH` resolution in the dispatch chain, registering a builtin named `ls`
guarantees that `ls` is handled in-process and never reaches `sys::process::spawn()`.

Builtin registration API:

```rust
// At construction time (ShellBuilder):
builder.builtin("ls", builtins::Registration { execute_func, content_func, ... })

// At runtime (on a live Shell):
shell.register_builtin("ls", registration)
```

`execute_func` has signature:
```rust
fn(ExecutionContext<'_, SE>, Vec<CommandArg>) -> BoxFuture<'_, Result<ExecutionResult, Error>>
```

`ExecutionContext` gives access to args, environment, open file descriptors, and the `Shell`
itself. A builtin can do anything: in-process logic, spawn its own process differently, return
a synthetic result, etc.

## What Is NOT Possible Without Forking

1. **A general pre-spawn hook for all external commands.** There is no callback between
   `$PATH` resolution and `tokio::process::Command::spawn()`.
2. **Replacing `sys::process::spawn()`.** It is `pub(crate)`, non-generic, non-trait-dispatched.
3. **Intercepting unknown command names at runtime.** If a command is not registered as a builtin,
   Brush will attempt to spawn it from `$PATH`. There is no "command not found" callback that
   returns a custom result rather than an error.

## Conclusion: The Correct Integration Strategy for clank.sh

**Register every clank command as a builtin.** Since the clank spec enumerates all commands
(builtins, core commands, AI commands), every command the shell is expected to handle can be
pre-registered by name. The `execute_func` for each registration dispatches to the appropriate
`Process` trait implementation.

The set of commands to register covers:

- All `parent-shell` builtins: `cd`, `exec`, `exit`, `export`, `source`, `unset`
- All `shell-internal` builtins: `alias`, `context`, `fg`, `bg`, `history`, `jobs`,
  `prompt-user`, `read`, `type`, `wait`, `which`
- All core commands: `ls`, `pwd`, `cat`, `cp`, `mv`, `rm`, `mkdir`, `touch`, `find`, `grep`,
  `sed`, `awk`, `sort`, `uniq`, `wc`, `head`, `tail`, `cut`, `tr`, `xargs`, `diff`, `patch`,
  `tee`, `printf`, `test`, `[`, `true`, `false`, `echo`, `sleep`, `jq`, `curl`, `wget`, `env`,
  `ps`, `kill`, `stat`, `file`, `man`
- All AI/platform commands: `ask`, `model`, `mcp`, `golem`, `grease`, `context`, `prompt-user`
- Dynamically: any command installed by `grease` is registered as a builtin at install time

Any command name not in this set falls through to Brush's `$PATH` resolution and OS spawning —
which on the WASM target will fail anyway (no `fork`/`exec`). On native, this is acceptable
behaviour for commands outside the clank surface (e.g. `git`, `vim`) that a developer might
invoke during local use.

### Implications

- There is no general escape hatch. A command name not pre-registered as a builtin that happens
  to exist on the host `$PATH` will be spawned as a real OS process. On native this is fine
  (and arguably useful). On WASM it will fail. The WASM target can disable `$PATH` resolution
  (by registering a "catch-all" builtin via a wrapper around `execute_via_external` that always
  returns `CommandNotFound`) if strict sandboxing is required — but this is a Phase 0 concern.

- `grease install` must register the new command as a builtin immediately after installation.
  `grease remove` must deregister it.

- The `brush-builtins` default set (`default_builtins()`) includes implementations of many of
  the core commands (e.g. `echo`, `pwd`, `read`, `type`). Where Brush's implementations are
  acceptable, clank can keep them. Where clank needs different behaviour (e.g. routing through
  the VFS, enforcing authorization policy), clank overrides with its own registration.

## Alternative: Fork brush-core to Add a ProcessSpawner Hook

If registering all commands by name proves unworkable — for example, because dynamically
installed packages arrive after startup and the registration mechanism becomes a bottleneck — a
minimal fork of `brush-core` that adds a `type ProcessSpawner: SpawnProcess` associated type to
`ShellExtensions` would solve the problem cleanly. The change is localised to three places in
`commands.rs`: replace the call to `sys::process::spawn()` with a call to
`SE::ProcessSpawner::spawn()`, thread `SE` through `execute_external_command`, and add a default
impl on `DefaultShellExtensions` that delegates to the existing `sys::process::spawn`. This is a
small, surgically targeted change that could be upstreamed as a PR to the Brush project.

This alternative should be kept in reserve. If the builtin-registration approach works cleanly
through Phase 3 (when `grease` introduces dynamic installs), the fork is unnecessary. If it
becomes painful, file an upstream PR first; fork only if the PR stalls.

## References

- `brush-core` 0.4.x source: https://github.com/reubeno/brush
- `brush-core` docs: https://docs.rs/brush-core/latest/brush_core/
