---
title: "Plan: Core commands batch 1 — filesystem and basic utilities"
date: 2026-03-07
author: agent
issue: dev-docs/issues/open/core-commands-batch-1.md
research:
  - dev-docs/research/core-commands-batch-1.md
designs: []
---

## Summary

Implement 16 core commands in `clank-builtins` following the established pattern:
clap-derived struct, `brush_core::builtins::Command` impl, registration in
`lib.rs`, golden tests for behavioural equivalence. All commands use `std::fs`,
`std::io`, `std::env`, and `tokio::time` only — no new crate dependencies.

`pwd` is excluded — brush's builtin already works correctly.

## Developer Feedback

Approved as proposed. No changes requested.

## Design Decisions and Rationale

### One file per command, same pattern as existing builtins

Each command gets its own `.rs` file in `clank-builtins/src/`. Struct with clap
derive, `Command` impl, registered in `lib.rs`. This is the established pattern
from `echo`, `true`, `false`, `ls`.

### No new dependencies

All 16 commands are implementable with `std` + `walkdir` (already present).
`touch` updates mtime by opening with `OpenOptions::append` — no `filetime`
crate needed.

### Minimal flag surface

Each command implements only the flags that LLMs commonly use. Rarely-used GNU
extensions are deferred. Specifically:

- `cat`: positional files, `-` for stdin. No `-n` (line numbers) in v1.
- `mkdir`: `-p` for parents.
- `rm`: `-r` for recursive, `-f` for force.
- `cp`: `-r` for recursive.
- `mv`: source(s) + destination only.
- `touch`: positional files only.
- `head`/`tail`: `-n N`.
- `wc`: `-l`, `-w`, `-c`.
- `sort`: `-r` (reverse), `-n` (numeric).
- `uniq`: `-c` (count).
- `sleep`: positional seconds (decimal).
- `env`: no flags — print all variables.
- `printf`: format string + args. Supports `%s`, `%d`, `%f`, `\\n`, `\\t`, `%%`.

### Error handling follows ls pattern

Errors are written to stderr with the command name prefix (e.g. `rm: foo: No such
file or directory`), exit code 1 on any error, 0 on full success. Partial success
(some args succeed, some fail) still exits 1 — matching POSIX behaviour.

### Golden tests per command

Each command gets at least one golden test in `clank/tests/golden/builtins/`.
Commands that operate on files use the setup mechanism to create temporary
files/directories first.

## Tasks

### Filesystem commands

- [ ] Implement `cat` in `clank-builtins/src/cat.rs` — read files to stdout,
      `-` reads stdin
- [ ] Implement `mkdir` in `clank-builtins/src/mkdir.rs` — create directories,
      `-p` for parents
- [ ] Implement `rm` in `clank-builtins/src/rm.rs` — remove files/directories,
      `-r` recursive, `-f` force
- [ ] Implement `cp` in `clank-builtins/src/cp.rs` — copy files, `-r` for
      recursive directory copy
- [ ] Implement `mv` in `clank-builtins/src/mv.rs` — move/rename files and
      directories
- [ ] Implement `touch` in `clank-builtins/src/touch.rs` — create empty files
      or update mtime

### Text processing commands

- [ ] Implement `head` in `clank-builtins/src/head.rs` — output first N lines
      (default 10), from files or stdin
- [ ] Implement `tail` in `clank-builtins/src/tail.rs` — output last N lines
      (default 10), from files or stdin
- [ ] Implement `wc` in `clank-builtins/src/wc.rs` — count lines, words, bytes;
      `-l`, `-w`, `-c` flags
- [ ] Implement `sort` in `clank-builtins/src/sort.rs` — sort lines from files
      or stdin; `-r` reverse, `-n` numeric
- [ ] Implement `uniq` in `clank-builtins/src/uniq.rs` — filter adjacent
      duplicates; `-c` count prefix

### Basic utilities

- [ ] Implement `sleep` in `clank-builtins/src/sleep.rs` — pause for N seconds
      (supports decimal)
- [ ] Implement `env` in `clank-builtins/src/env.rs` — print all environment
      variables as KEY=VALUE
- [ ] Implement `printf` in `clank-builtins/src/printf.rs` — formatted output
      with `%s`, `%d`, `%f`, `\n`, `\t`, `%%`

### Registration and tests

- [ ] Register all 14 new commands in `clank-builtins/src/lib.rs`
- [ ] Add golden tests for each command in `clank/tests/golden/builtins/`
- [ ] Add unit tests in `clank-builtins` for non-trivial logic (printf formatting,
      wc counting, sort comparisons)
- [ ] Verify all tests pass: `cargo test`
- [ ] Verify `cargo clippy --all-targets -- -D warnings` passes

## Acceptance Tests

1. `cargo test` passes with zero failures.
2. `cargo clippy --all-targets -- -D warnings` passes.
3. Every new command has at least one golden test demonstrating correct output.
4. All commands compile without `nix`, `libc`, or OS-specific dependencies
   (WASM-compatible by construction — only `std` and `walkdir` used).
