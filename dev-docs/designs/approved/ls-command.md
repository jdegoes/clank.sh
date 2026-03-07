---
title: ls Command — Realized Design
date: 2026-03-07
author: agent
---

## Overview

This document records the `ls` internal implementation as actually built.
It supersedes any prior approved design for this area (none existed).

---

## What Was Built

`ls` implemented as an internal `clank-builtins` command using `std::fs` and
`walkdir`. No OS process is spawned. All code compiles to `wasm32-wasip2`.
Behavioural equivalence with the OS `ls` is enforced by live OS comparison
tests and golden regression tests.

---

## New Files

```
clank-builtins/src/ls.rs
clank/tests/golden/fixtures/ls-test-dir/   ← committed fixture directory
clank/tests/golden/ls/ls-plain.yaml
clank/tests/golden/ls/ls-a.yaml
clank/tests/golden/ls/ls-recursive.yaml
```

---

## Named Types

Per AGENTS.md code conventions, all types are named — no anonymous tuples:

```rust
struct ListEntry {
    display_name: String,   // filename shown in output
    path: PathBuf,          // full path for metadata reads
    is_dir: bool,
    is_symlink: bool,
    size_bytes: u64,
    modified: Option<SystemTime>,
}

struct DisplayOptions {
    show_hidden: bool,   // -a
    long_format: bool,   // -l
    recursive: bool,     // -R
}
```

---

## Flags Supported

| Flag | Behaviour |
|---|---|
| (none) | Entry names, one per line, alphabetically sorted |
| `-a` | Include hidden entries; prepends `.` and `..` first |
| `-l` | Long format: permissions, size, date, name |
| `-R` | Recursive; root entries printed directly, subdirs with `path:` headers |
| Combinations | `-la`, `-al`, `-lR`, `-Ra`, `-lRa` etc. |

---

## Output Format

**Short format:** one entry per line, alphabetically sorted. Matches OS `ls`
when stdout is not a TTY.

**Long format (`-l`):**
```
-rw-r--r--  1 501  20      0 2026  3  7 12:00 a-file.txt
drwxr-xr-x  1 501  20    160 2026  3  7 12:00 sub-dir
```

**`-a` behaviour:** `.` and `..` are prepended before all other entries,
matching OS `ls -a` exactly.

**`-R` behaviour:** root directory contents printed directly (no header),
then each subdirectory printed with a blank line separator and `path:` header.
This matches macOS/Linux `ls -R` behaviour exactly — confirmed by the OS
equivalence tests.

---

## WASM Compatibility

| Concern | Solution |
|---|---|
| Directory listing | `std::fs::read_dir` — WASI-supported |
| Recursive traversal | `walkdir 2.5.0` — no `nix`, no `libc` |
| Permission bits | `#[cfg(unix)]` `MetadataExt::mode()` / `#[cfg(not(unix))]` fallback `"----------"` |
| Owner/group | `#[cfg(unix)]` numeric UID/GID / `#[cfg(not(unix))]` fallback `"-"` |
| Timestamps | `secs_to_datetime()` — pure arithmetic, no OS time API, WASM-compatible |

`libc` was explicitly avoided — owner/group display uses numeric UIDs/GIDs
rather than resolving names, which avoids the `libc::getpwuid` / `libc::getgrgid`
dependency.

---

## Behavioural Equivalence Tests

Three live OS equivalence tests in `clank/tests/builtins.rs`:

| Test | What it verifies |
|---|---|
| `ls_plain_matches_os` | Plain `ls` output identical to OS `ls` |
| `ls_a_matches_os` | `ls -a` output identical to OS `ls -a` |
| `ls_recursive_matches_os` | `ls -R` output identical to OS `ls -R` |

Each test runs the real OS `ls` via `std::process::Command` and clank's
internal `ls` via `assert_cmd`, then asserts the outputs are equal. These
tests caught a real bug during implementation: our `-R` was printing a root
directory header that OS `ls -R` does not print.

---

## Golden Tests

Three golden YAML tests in `clank/tests/golden/ls/`, using a committed
fixture directory with stable contents:

```
ls-test-dir/
├── a-file.txt
├── b-file.txt
├── .hidden-file
└── sub-dir/
    └── nested-file.txt
```

Golden tests lock output against future regressions. OS equivalence tests
verify correctness against the live OS command.

---

## Deviations from the Approved Plan

- The plan said owner/group would attempt to resolve names via `libc`. During
  implementation this was changed to numeric UID/GID only, avoiding the
  `libc` dependency entirely. This is a better outcome — fully WASM-compatible
  with zero additional deps.
- The `-l` long format timestamp uses a hand-rolled `secs_to_datetime`
  function rather than `chrono` — keeping deps minimal and WASM-compatible.

---

## Test Count

54 tests total, all passing. Clippy clean.
