---
title: Implement Native Shell Scaffold (Hello World)
date: 2026-03-07
author: agent
issue: dev-docs/issues/open/native-shell-scaffold.md
research:
  - dev-docs/research/brush-embeddability.md
designs: []
---

## Summary

Establish the initial Rust workspace and embed `brush-core` to produce a runnable native binary with a minimal interactive REPL. This is the foundational scaffold upon which all future clank.sh work depends.

The scope is deliberately minimal: type `echo hello`, see `hello`, type `exit` or Ctrl-D, shell exits cleanly. Nothing more.

## Design Decisions and Rationale

### No `brush-interactive` for now

`brush-interactive` brings `nix` as a dependency and a full `reedline` readline stack. Neither is needed for hello-world. A plain stdin read loop against `brush-core` is sufficient and keeps the dependency surface small. Readline and tab completion are future concerns.

### No `brush-shell` binary dependency

clank.sh embeds `brush-core` directly. It does not depend on the `brush-shell` binary crate — that crate includes opinionated CLI argument parsing, config file loading, and event tracing that clank.sh will replace with its own implementations.

### Workspace layout

A Cargo workspace with a single binary crate `clank` is sufficient for now. Additional library crates (e.g. `clank-core`, `clank-http`) will be added as the design calls for them. Starting with a workspace root makes that refactor trivial later.

### Tokio runtime

`brush-core`'s `Shell::new` and `Shell::run_string` are `async`. A `tokio::main` macro entry point with `#[tokio::main]` and `features = ["full"]` is the simplest correct approach.

### `no_profile` and `no_rc` for now

We skip sourcing `/etc/profile` and `~/.bashrc` in the initial scaffold. clank.sh will have its own initialization story; inheriting the developer's bash config at this stage would add noise and potential failures.

## Developer Feedback

- **WASM upstream contribution**: The research doc noted contributing upstream to `brush-core` to make `nix` optional as a possible WASM approach. Developer is strongly opposed to this. We must not plan for or depend on upstream contributions to Brush. If the `nix` dependency blocks WASM, we resolve it within this project (fork, patch, or replace the affected layer entirely) — never by pushing changes upstream.
- **Acceptance tests**: Developer requested that acceptance tests be implemented as actual test code, not just a manual checklist.

## Acceptance Tests

1. `cargo build` succeeds with no errors on macOS and Linux.
2. `cargo run` starts the shell and displays a prompt.
3. Typing `echo hello` and pressing Enter prints `hello`.
4. Typing `ls` lists the current directory.
5. Typing `exit` exits with code 0.
6. Pressing Ctrl-D exits with code 0.
7. `echo $?` after a successful command prints `0`.
8. `echo $?` after a failing command (e.g. `false`) prints `1`.

## Tasks

- [ ] Create `Cargo.toml` at the workspace root with `[workspace]` and `members = ["clank"]`
- [ ] Create `clank/Cargo.toml` binary crate with dependencies: `brush-core`, `brush-builtins`, `tokio`
- [ ] Create `clank/src/main.rs` with a `#[tokio::main]` entry point
- [ ] Instantiate `brush_core::Shell` via `CreateOptions` with `interactive = true`, `no_profile = true`, `no_rc = true`, `shell_name = Some("clank".into())`
- [ ] Register default builtins via `brush-builtins`
- [ ] Implement a read-eval-print loop: print a prompt (`$ `), read a line from stdin, call `shell.run_string(line, &params).await`, handle EOF (Ctrl-D) and `exit` cleanly
- [ ] Write integration tests in `clank/tests/repl.rs` covering acceptance tests 3, 4, 7, and 8 (command execution and exit codes) using `assert_cmd` or equivalent
- [ ] Verify all acceptance tests pass (`cargo test` green, manual checks for 2, 5, 6)
