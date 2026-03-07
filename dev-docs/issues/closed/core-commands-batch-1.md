---
title: "Core commands batch 1 — filesystem and basic utilities"
date: 2026-03-07
author: agent
closed: 2026-03-07
plan: dev-docs/plans/done/core-commands-batch-1.md
---

# Core commands batch 1 — filesystem and basic utilities

## Problem

clank.sh currently implements only `echo`, `true`, `false`, and `ls` as internal
WASM-compatible commands in `clank-builtins`. The README specifies a much larger
set of core commands that must be available as `execution-scope: subprocess`
builtins. Without these, the shell is not usable for basic file manipulation,
text processing, or scripting — and LLMs cannot operate the shell the way they
would a real Linux environment.

## Scope

This issue covers the next batch of high-value commands that unblock real shell
usage. Grouped by priority:

### Filesystem (highest priority — unblocks basic file workflows)

- `cat` — concatenate and display files
- `mkdir` — create directories
- `rm` — remove files and directories
- `cp` — copy files and directories
- `mv` — move/rename files and directories
- `touch` — create empty files or update timestamps
- `pwd` — print working directory

### Text processing (high priority — unblocks scripting and pipe composition)

- `head` — output first N lines
- `tail` — output last N lines
- `wc` — word, line, character count
- `sort` — sort lines
- `uniq` — filter duplicate lines

### Basic utilities

- `sleep` — delay for a specified duration
- `env` — print environment variables
- `printf` — formatted output

## Constraints

- All implementations must compile to `wasm32-wasip2` (no `nix`, `libc`, OS
  process spawning, or Unix-only system calls).
- Each command must produce output behaviourally equivalent to the corresponding
  OS command for the same inputs (golden tests enforce this).
- Follow existing patterns in `clank-builtins` for registration, error handling,
  and code structure.

## Not in scope

- `grep`, `find`, `sed`, `awk`, `jq`, `curl` — these are more complex commands
  that warrant their own issues.
- `ps`, `kill` — depend on the process table, which is a separate concern.
- `stat`, `file`, `man` — depend on metadata/help infrastructure not yet built.
