---
title: "Plan: Phase -1 — Native-only foundation"
date: 2026-03-06
author: agent
issue: "dev-docs/issues/open/phase-neg1-native-foundation.md"
research:
  - "dev-docs/research/spec-analysis-and-implementation-gaps.md"
  - "dev-docs/research/virtual-filesystem-driver-options.md"
  - "dev-docs/research/brush-command-dispatch.md"
designs:
  - "dev-docs/designs/proposed/workspace-and-crate-structure.md"
---

# Plan: Phase -1 — Native-only foundation

## Originating Issue

`dev-docs/issues/open/phase-neg1-native-foundation.md` — no implementation exists; the project
needs a native shell binary that starts and runs basic commands, with the core architectural
abstractions in place for all later phases.

## Research Consulted

- `dev-docs/research/spec-analysis-and-implementation-gaps.md` — full spec analysis; identifies
  the `Process` trait, `clank-http` seam, and Brush integration as Phase -1 concerns.
- `dev-docs/research/virtual-filesystem-driver-options.md` — VFS research; confirms the VFS is
  out of scope for this phase (stub only).
- `dev-docs/research/brush-command-dispatch.md` — full investigation of brush-core's command
  dispatch internals and available intercept mechanisms; resolves the key uncertainty in this plan.

## Design Referenced

- `dev-docs/designs/proposed/workspace-and-crate-structure.md` — specifies the crate layout,
  dependency graph, and phase mapping this plan implements. **Note: pending human approval.**

## Developer Feedback

_To be filled in after consultation._

## Approach

Stand up the Cargo workspace with the full crate skeleton from the workspace design. Integrate
Brush (`brush-core` 0.4.x, `brush-parser` 0.3.x, `brush-builtins`) into `clank-shell`, replacing
`brush-interactive` with a minimal tokio-driven stdin/stdout loop. Define the internal `Process`
trait with stub implementations for each process type. Wire `NativeHttpClient` into the binary.
All stub process types return a clear, stable error string rather than panicking.

### Key Brush integration notes

- `brush-core` provides `Shell`, `ShellBuilder`, and `CreateOptions`. Shell creation is async
  (`Shell::new(&options).await`).
- Builtins are registered via `builtins::Registration` on the `Shell` struct's `builtins`
  `HashMap`. The `brush-builtins` crate provides `default_builtins()` and a `ShellBuilderExt`
  extension trait for convenience.
- `brush-core`'s `nix` dependency compiles fine on native; exclusion is deferred to Phase 0.
- `brush-core` uses `tokio` for async throughout — the shell's main loop must be inside a
  `#[tokio::main]` runtime.

### How clank intercepts Brush's command dispatch (resolved)

Research into the brush-core source (`dev-docs/research/brush-command-dispatch.md`) determined
that **there is no general pre-spawn hook** available to embedders. `sys::process::spawn()` is
`pub(crate)` and non-trait-dispatched. The `interfaces` module exports only `KeyBindings`. The
`ShellExtensions` trait carries only `ErrorFormatter`.

**The only in-process intercept mechanism is builtin registration by name.** Brush's dispatch
chain checks builtins before `$PATH` resolution, so any command name registered as a builtin is
handled entirely in-process and never reaches `sys::process::spawn()`.

**Strategy: register every clank command as a builtin.** The full set of commands in the clank
spec is finite and enumerated. Each registration's `execute_func` dispatches to the appropriate
`Process` trait implementation. The `brush-builtins` default set covers several core commands
(`echo`, `pwd`, `read`, `type`, etc.) with acceptable implementations; clank keeps those and
overrides only where its own behaviour differs (VFS routing, authorization policy, etc.).

Commands not pre-registered fall through to Brush's `$PATH` resolution and real OS spawning. On
native this is acceptable for host commands outside the clank surface (e.g. `git`, `vim`). On
WASM (Phase 0), all such fallthrough will fail at the OS spawn level — acceptable, since the
WASM sandbox is supposed to prevent arbitrary host command execution.

**Future escape hatch:** if dynamic `grease` installs in Phase 3 make per-name registration
cumbersome, a minimal upstream PR to `brush-core` adding a `ProcessSpawner` associated type to
`ShellExtensions` would solve it cleanly. Fork only if the PR stalls.

## Tasks

- [ ] Create `rust-toolchain.toml` pinning Rust stable (≥ 1.87.0, the current Brush MSRV)
- [ ] Create workspace `Cargo.toml` with all eight crate members:
      `clank`, `clank-shell`, `clank-http`, `clank-vfs`, `clank-ask`, `clank-manifest`,
      `clank-golem`, `clank-grease`
- [ ] Create stub `Cargo.toml` and `lib.rs` (or `main.rs`) for each crate; confirm dependency
      graph matches the workspace design (no forbidden edges)
- [ ] Add `brush-core`, `brush-parser`, `brush-builtins`, `tokio`, `async-trait`, `anyhow` to
      `clank-shell`'s dependencies
- [ ] Add `reqwest` (with `rustls-tls`, not `native-tls`) to `clank-http`'s dependencies
- [ ] Define `Process` trait in `clank-shell`:
      ```rust
      #[async_trait]
      pub trait Process: Send + Sync {
          async fn run(&self, ctx: ProcessContext) -> ProcessResult;
      }
      ```
      with `ProcessContext` (args, env, stdio handles) and `ProcessResult` (exit code, error)
- [ ] Implement stub `Process` types: `BuiltinProcess`, `ScriptProcess`, `PromptProcess`,
      `GolemAgentProcess` — each returns exit code `1` and writes
      `"clank: <type>: not yet implemented\n"` to stderr
- [ ] Implement `HttpClient` trait and `NativeHttpClient` in `clank-http`:
      ```rust
      #[async_trait]
      pub trait HttpClient: Send + Sync {
          async fn send(&self, req: Request) -> Result<Response, HttpError>;
      }
      ```
      `HttpError` enum: `ConnectionFailed`, `Timeout`, `NonSuccessResponse { status: u16,
      body: String }`, `Tls`
- [ ] Integrate Brush into `clank-shell`:
      - Create `ClankShell` struct wrapping `brush_core::Shell<ClankExtensions>` where
        `ClankExtensions` implements `brush_core::extensions::ShellExtensions`
      - Build the shell via `ShellBuilder`, registering `brush-builtins` defaults first, then
        overriding with clank's own registrations for any command where behaviour differs
      - Register each clank command as a `builtins::Registration` whose `execute_func` dispatches
        to the appropriate `Process` trait implementation:
        - `parent-shell` builtins: `cd`, `exec`, `exit`, `export`, `source`, `unset`
        - `shell-internal` builtins: `alias`, `context`, `fg`, `bg`, `history`, `jobs`,
          `prompt-user`, `read`, `type`, `wait`, `which`
        - Core commands: `ls`, `pwd`, `cat`, `cp`, `mv`, `rm`, `mkdir`, `touch`, `find`, `grep`,
          `sed`, `awk`, `sort`, `uniq`, `wc`, `head`, `tail`, `cut`, `tr`, `xargs`, `diff`,
          `patch`, `tee`, `printf`, `test`, `[`, `true`, `false`, `echo`, `sleep`, `jq`,
          `curl`, `wget`, `env`, `ps`, `kill`, `stat`, `file`, `man`
        - AI/platform commands: `ask`, `model`, `mcp`, `golem`, `grease`
        - At this phase, all of the above (except the Brush-provided defaults for `echo`, `pwd`,
          `read`, `type`, `true`, `false`) are stub implementations
      - Implement minimal interactive loop: read line from stdin, pass to Brush for execution,
        print result — no readline, no history, no prompt formatting yet
- [ ] Implement `clank-vfs` stub: `Vfs` trait with `read_file`, `read_dir`, `stat`, `exists`;
      `RealFs` implementation delegating to `std::fs`; no virtual mounts yet
- [ ] Wire up `clank` binary:
      - `#[tokio::main]` entry point
      - Construct `Arc<dyn HttpClient>` as `NativeHttpClient`
      - Construct `Arc<dyn Vfs>` as `RealFs`
      - Inject into `ClankShell`, start interactive loop
- [ ] Add `.cargo/config.toml` (native default target; WASM config stubbed but commented out)
- [ ] Confirm `cargo build` passes with zero errors
- [ ] Confirm `cargo clippy --all-targets -- -D warnings` passes with zero warnings
- [ ] Confirm `cargo fmt --check` passes
- [ ] Write integration tests (see Acceptance Tests below) in `clank/tests/` using
      `assert_cmd` to spawn the binary
- [ ] Confirm `cargo test` passes with all integration tests green

## Acceptance Tests

All of the following must pass as automated `cargo test` integration tests. Tests spawn the
`clank` binary as a subprocess, send commands via stdin, and assert on stdout, stderr, and exit
code. Use the `assert_cmd` crate.

### Build and static checks

```
cargo build                                       # exits 0
cargo clippy --all-targets -- -D warnings         # exits 0, zero diagnostics
cargo fmt --check                                 # exits 0
cargo test                                        # exits 0, all tests pass
```

### Crate layering (verified by inspection of Cargo.toml files)

- `clank-shell/Cargo.toml` does NOT list `clank-ask`, `clank-golem`, or `clank-grease`
- `clank-manifest/Cargo.toml` has no internal crate dependencies
- `clank-http/Cargo.toml` has no internal crate dependencies
- No `Cargo.toml` other than `clank-http` lists `reqwest` as a dependency
- `clank/Cargo.toml` depends on all other crates; no other crate does

### Shell behaviour (automated integration tests)

Each test below is a separate `#[test]` function using `assert_cmd::Command`.

| # | Input (stdin) | Expected stdout | Expected exit code |
|---|---|---|---|
| 1 | `echo "hello world"` | `hello world` | 0 |
| 2 | `export FOO=bar && echo $FOO` | `bar` | 0 |
| 3 | `pwd` | matches `std::env::current_dir()` | 0 |
| 4 | `cd /tmp && pwd` | `/tmp` (or `realpath` equivalent) | 0 |
| 5 | `echo "hello" \| cat` | `hello` | 0 |
| 6 | `echo "hello" > /tmp/clank-t1 && cat /tmp/clank-t1` | `hello` | 0 |
| 7 | `false && echo "no"` | _(empty)_ | 1 |
| 8 | `false \|\| echo "yes"` | `yes` | 0 |
| 9 | `false ; echo "yes"` | `yes` | 0 |
| 10 | `cat <<EOF\nhello\nEOF` | `hello` | 0 |
| 11 | script file: `#!/bin/sh\necho "from script"` | `from script` | 0 |
| 12 | `ls /nonexistent-clank-test-path` | _(stderr non-empty)_ | non-0 |

### Process trait stub behaviour (automated integration tests)

| # | Input | Expected stderr contains | Expected exit code |
|---|---|---|---|
| 13 | `ask "hello"` | `not yet implemented` | non-0 |
| 14 | `some-agent some-method` (non-existent command) | any error message | non-0 |

Test 13 validates the `PromptProcess` stub. Test 14 validates that unresolvable commands produce
an error and do not crash.

### Brush known-gap graceful failure (automated integration tests)

| # | Input | Expectation |
|---|---|---|
| 15 | `coproc { echo hi; }` | exits non-0, does not panic, process terminates cleanly |
| 16 | `select x in a b c; do echo $x; done` | exits non-0, does not panic, process terminates cleanly |

"Does not panic" is validated by asserting the process exits with any code rather than via a
signal (i.e. `assert_cmd`'s `.failure()` rather than a signal-based termination).

## Notes on Implementation Order

The suggested implementation order within this phase:

1. Workspace and crate skeletons first — unblocks parallel work on all crates.
2. `clank-http` and `clank-vfs` stubs — no Brush dependency, can be done in isolation.
3. `Process` trait and stub implementations in `clank-shell`.
4. Brush integration in `clank-shell` — the intercept mechanism is now resolved (builtin
   registration by name); no further uncertainty. Begin with a minimal set of registrations
   (`echo`, `pwd`, `ls`, `cd`) and expand to the full list once the pattern is confirmed working.
5. `clank` binary wiring.
6. Integration tests last, written against the running binary.
