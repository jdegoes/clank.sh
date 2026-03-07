---
title: Native Shell Scaffold — Realized Design
date: 2026-03-07
author: agent
---

## Overview

This document records the design as actually realized by the native shell scaffold implementation. It supersedes any prior approved design for this area (none existed). The code is the ground truth; this document records the decisions and structure at the point of completion.

## What Was Built

A Cargo workspace with a single binary crate `clank` that embeds `brush-core` and runs a minimal interactive REPL on native Rust. The binary accepts bash-compatible commands on stdin, executes them via the Brush interpreter, and prints results to stdout.

## Workspace Structure

```
clank.sh/
├── Cargo.toml          # workspace root: members = ["clank"], resolver = "2"
└── clank/
    ├── Cargo.toml      # binary crate
    ├── src/
    │   └── main.rs     # entry point + REPL loop
    └── tests/
        └── repl.rs     # integration tests
```

## Dependencies

### Runtime

| Crate | Version | Role |
|---|---|---|
| `brush-core` | 0.4.0 | Embeddable bash-compatible shell interpreter |
| `brush-builtins` | 0.1.0 | Default builtin command set (BashMode) |
| `tokio` | 1 (features = full) | Async runtime required by `brush-core` |

### Dev / Test

| Crate | Version | Role |
|---|---|---|
| `assert_cmd` | 2 | Spawn the binary in integration tests and assert on output |
| `predicates` | 3 | Composable output matchers used with `assert_cmd` |

## Shell Initialisation

`Shell` is constructed via the `ShellBuilder` pattern (from `brush-core`, using the `bon` derive macro internally). The `ShellBuilderExt` trait from `brush-builtins` adds the `default_builtins` method to the builder.

```rust
let shell = Shell::builder()
    .default_builtins(BuiltinSet::BashMode)
    .shell_name("clank".to_string())
    .no_profile(true)
    .no_rc(true)
    .build()
    .await?;
```

Key decisions:
- `BashMode` builtins — full bash-compatible builtin set, not the narrower `ShMode`.
- `no_profile(true)` — skips `/etc/profile`. clank.sh will have its own init story.
- `no_rc(true)` — skips `~/.bashrc`. Same reason.
- `shell_name("clank")` — sets `$0` and the shell identity inside the interpreter.
- `interactive` is not set (defaults to `false`) — the REPL is implemented manually rather than delegating to `brush-interactive`, which carries `nix` and `reedline` dependencies unnecessary at this stage.

## REPL Loop

The REPL is a plain synchronous stdin reader wrapped in an async function:

```rust
async fn run_repl(mut shell: Shell) {
    let stdin = io::stdin();
    let mut lines = stdin.lock().lines();
    loop {
        eprint!("$ ");
        let _ = io::stderr().flush();
        match lines.next() {
            None => break,                        // EOF / Ctrl-D
            Some(Err(e)) => { eprintln!(...); break; }
            Some(Ok(line)) => {
                let trimmed = line.trim();
                if trimmed.is_empty() { continue; }
                if trimmed == "exit" { break; }
                let params = shell.default_exec_params();
                shell.run_string(trimmed, &params).await?;
            }
        }
    }
}
```

Key decisions:
- **Prompt is written to stderr**, not stdout. This keeps stdout clean for test assertions and for future use of stdout as a pipe target.
- **`exit` is handled in the loop**, not by the shell's own `exit` builtin. This is a temporary shortcut. In future, `brush-core`'s execution result will carry an exit request signal that the loop should observe.
- **`stdin.lock().lines()`** is a synchronous blocking read inside the tokio runtime. This is acceptable for the scaffold because the REPL is the only task running. It will need revisiting when the shell gains concurrent async work (e.g. background jobs, AI calls).
- **`shell.default_exec_params()`** returns a default `ExecutionParameters` value that Brush uses to configure execution context per `run_string` call.

## What Was Intentionally Deferred

| Concern | Rationale |
|---|---|
| `brush-interactive` (readline, tab completion) | Adds `nix` + `reedline`; not needed for scaffold |
| Observing `exit` builtin's exit code | Brush's exit result type needs investigation; deferred to next issue |
| `interactive = true` on the shell | Brush's interactive mode hooks into readline; deferred with readline |
| WASM / `wasm32-wasip2` target | `brush-core` depends on `nix` (non-optional); entire WASM seam deferred |
| Upstream contribution to `brush-core` | Ruled out by developer; any WASM fix will be internal to clank.sh |
| Signal handling (Ctrl-C) | Requires `nix` or platform-specific code; deferred |
| Background jobs | Requires the async process model design; deferred |

## Acceptance Tests

All 4 automated acceptance tests pass (`cargo test`):

| Test | What it checks |
|---|---|
| `echo_hello` | `echo hello` prints `hello` on stdout |
| `ls_produces_output` | `ls` produces non-empty stdout |
| `exit_code_zero_after_success` | `$?` is `0` after `true` |
| `exit_code_one_after_failure` | `$?` is `1` after `false` |

Manual acceptance tests also pass:
- `cargo build` succeeds on macOS (aarch64)
- `cargo run` starts the shell and shows `$ ` prompt
- `exit` and Ctrl-D both terminate cleanly with exit code 0

## Deviations from the Approved Plan

None. All tasks in the approved plan were completed as specified.
