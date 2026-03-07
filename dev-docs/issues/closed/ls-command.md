---
title: ls is not an internal command — still spawns a real OS process
date: 2026-03-07
author: agent
---

## Problem

`ls` is one of the most fundamental commands an AI agent will use to navigate and
understand the filesystem. It is currently not registered in `clank-builtins` and
therefore falls through to Brush's OS process spawn path — a real `/bin/ls` is
executed on the host. This violates the core architecture of clank.sh.

From the README:

> "The consequence is a true sandbox: the AI can only do what is installed, and
> cannot reach the underlying OS."

A spawned `/bin/ls` reaches outside the sandbox. In a Golem-deployed WASM component,
no such process exists — the command would silently fail or produce undefined
behaviour.

## Capability Gap

`ls` is not implemented as an internal clank command. There is no Rust implementation
in `clank-builtins`. The architecture established by the internal process trait task
(which proved the pattern with `echo`, `true`, `false`) is not yet extended to
filesystem commands.

## What `ls` Must Do

Per the README's LLM-legibility principle:

> "Directory structure, command names, flags, exit codes — all behave as an LLM expects."

An AI agent trained on Linux will invoke `ls` with the following common forms:

- `ls` — list current directory
- `ls /some/path` — list a specific path
- `ls -l` — long format (permissions, size, timestamps)
- `ls -a` — include hidden files (dotfiles)
- `ls -la` / `ls -al` — combined
- `ls -R` — recursive listing

Exit codes must conform to the README's exit code table:
- `0` — success
- `1` — general error (e.g. path does not exist)

Output format must be LLM-legible — matching what an LLM expects from standard
`ls` output. Exact byte-for-byte POSIX compliance is not required, but the output
must not surprise a well-trained model.

## Why This Must Be Solved

`ls` is the single most-used filesystem command in shell sessions. An AI agent
navigating a project, inspecting a directory, or verifying the result of a file
operation will reach for `ls` constantly. Without an internal implementation:

- The sandbox does not exist for filesystem inspection
- Golem deployment is broken for this command
- AI agents operating the shell encounter unexpected failures or OS-level side effects

## Implementation Constraint: WASM Compatibility

All dependencies used in `clank-builtins` must compile to `wasm32-wasip2`. This
rules out any crate that depends on `nix`, `libc`, Unix system calls, or OS
process spawning.

`uutils/coreutils` (`uu_ls`) was evaluated as a candidate implementation. It is
**not usable** — `uucore` has `nix = "^0.30"` as a non-optional dependency, along
with `libc`, `terminal_size`, and Unix-specific filesystem APIs. It will not compile
to WASM.

The implementation must use only WASM-compatible primitives:
- `std::fs` — filesystem traversal (`read_dir`, `DirEntry`, `Metadata`)
- `std::os::unix::fs::MetadataExt` gated behind `#[cfg(unix)]` for permission bits
- `walkdir` (WASM-compatible) for recursive listing if needed

## Acceptance Condition

`ls`, `ls /path`, `ls -l`, `ls -a`, and `ls -la` all execute as internal Rust
functions in `clank-builtins` with no OS process spawned. All dependencies compile
to `wasm32-wasip2`.

Behavioural equivalence with the OS `ls` is enforced by golden tests per the
project-wide rule in `AGENTS.md`.
