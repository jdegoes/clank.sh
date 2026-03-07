---
title: "Research: Core commands batch 1 — filesystem and basic utilities"
date: 2026-03-07
author: agent
---

# Research: Core commands batch 1

## Objective

Determine the implementation approach, flag surface, and WASM compatibility
constraints for the 17 commands in core-commands-batch-1.

## Existing patterns in clank-builtins

Every command:

1. Lives in its own file in `clank-builtins/src/` (e.g. `echo.rs`).
2. Defines a `#[derive(Debug, Parser)]` struct with clap fields for flags/args.
3. Implements `brush_core::builtins::Command` with `type Error = brush_core::Error`.
4. Uses `context.stdout()` / `context.stderr()` for I/O.
5. Returns `ExecutionResult::success()` or `ExecutionResult::new(code)`.
6. Is registered in `lib.rs` via `shell.register_builtin(name, builtin::<Cmd>())`.
7. Has golden tests in `clank/tests/golden/builtins/`.

## Command-by-command analysis

### Filesystem commands

| Command | Key flags | Implementation notes |
|---------|-----------|---------------------|
| `cat`   | (files...) | `std::fs::read_to_string` per file, write to stdout. Handle `-` as stdin. |
| `mkdir` | `-p` (parents) | `std::fs::create_dir` / `create_dir_all`. |
| `rm`    | `-r` (recursive), `-f` (force) | `std::fs::remove_file` / `remove_dir_all`. `-f` suppresses errors for missing files. |
| `cp`    | `-r` (recursive) | `std::fs::copy` for files. For `-r`, walk with `walkdir` (already a dependency). |
| `mv`    | (src dest) | `std::fs::rename`. Cross-device: fall back to copy+remove. |
| `touch` | (files...) | Create if missing (`File::create`). If exists, update mtime via `filetime` crate or just open+close. |
| `pwd`   | (none) | `std::env::current_dir()`. Note: brush already has `pwd` as a builtin — we may want to override or skip. |

### Text processing commands

| Command | Key flags | Implementation notes |
|---------|-----------|---------------------|
| `head`  | `-n N` (default 10) | Read lines from stdin or files, output first N. |
| `tail`  | `-n N` (default 10) | Read all lines, output last N. No `-f` (follow) — requires OS inotify. |
| `wc`    | `-l`, `-w`, `-c` | Count lines, words, chars/bytes. Default: all three. |
| `sort`  | `-r` (reverse), `-n` (numeric) | Read all lines, sort, output. |
| `uniq`  | `-c` (count) | Filter adjacent duplicates. |

### Basic utilities

| Command | Key flags | Implementation notes |
|---------|-----------|---------------------|
| `sleep` | (seconds) | `tokio::time::sleep` — async, WASM-compatible. |
| `env`   | (none) | `std::env::vars()` — print all. |
| `printf`| format args | Subset of printf: `%s`, `%d`, `%f`, `\\n`, `\\t`. Full printf is complex; start with common cases. |

## WASM compatibility

All 17 commands use only `std::fs`, `std::io`, `std::env`, and `tokio::time` —
all available on `wasm32-wasip2`. No new non-WASM dependencies are needed.

The one exception is `touch` for updating modification times on existing files:
`std::fs` has no `set_modified` on all platforms. Options:
- Use `filetime` crate (pure Rust, WASM-compatible).
- Or simply open the file for write without truncating (`OpenOptions::append`),
  which updates mtime on most filesystems.
- Recommendation: use `OpenOptions` approach — zero new dependencies.

## pwd — brush conflict

Brush registers its own `pwd` builtin. Overriding it via `register_builtin` works
(same pattern as `echo` override). However, `pwd` is already tested and working
via brush. Decision: **skip `pwd`** — it already works correctly. No override needed.

## printf complexity

Full POSIX `printf` is a format-string interpreter. For v1, implement the subset
that LLMs actually use: `%s`, `%d`, `\\n`, `\\t`, `%%`. Escape sequences in the
format string. This covers 95%+ of real usage.

## Dependencies

No new crate dependencies needed. `walkdir` (already used by `ls`) covers
recursive `cp`. `std::fs` and `std::io` cover everything else.

## Golden test strategy

Each command gets golden tests verifying behavioural equivalence with OS commands.
Tests live in `clank/tests/golden/builtins/`. For commands that operate on files,
use the setup mechanism to create temp files via shell commands first.
