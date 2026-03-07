---
title: ls Internal Implementation — WASM-Compatible Approach
date: 2026-03-07
author: agent
---

## Purpose

Determine the correct implementation strategy for `ls` as an internal
`clank-builtins` command that compiles to `wasm32-wasip2`.

---

## Candidate Libraries — Ruled Out

### `uutils/coreutils` (`uu_ls`)

Ruled out. `uucore` has `nix = "^0.30"` as a non-optional normal dependency,
along with `libc`, `terminal_size`, and Unix-specific filesystem APIs. Will not
compile to `wasm32-wasip2`.

### Any other existing `ls` clone

Surveyed: `lsplus`, `ricat`, `oreutils`. All either learning projects, not
embeddable as libraries, or have Unix-only dependencies. None are suitable.

---

## Implementation Strategy: `std::fs` + `walkdir`

### `std::fs` (WASM-compatible)

`std::fs` is part of the Rust standard library and is supported on
`wasm32-wasip2` via the WASI filesystem interface. All operations needed for
`ls` are available:

| Operation | API |
|---|---|
| List directory | `std::fs::read_dir(path)` |
| File metadata | `DirEntry::metadata()` |
| File type | `FileType::is_dir()`, `is_file()`, `is_symlink()` |
| File size | `Metadata::len()` |
| Modified time | `Metadata::modified()` |
| Permissions (Unix) | `std::os::unix::fs::MetadataExt::mode()` — gated behind `#[cfg(unix)]` |

### `walkdir` (WASM-compatible)

`walkdir` 2.5.0 depends only on `same-file` and `winapi-util` (Windows only).
No `nix`, no `libc`. Compiles to `wasm32-wasip2`.

Used for `-R` (recursive) listing — `WalkDir::new(path).min_depth(1)`.

---

## `ls` Behaviour to Implement

Based on what an LLM will most commonly invoke, and the AGENTS.md behavioural
equivalence rule:

### Flags

| Flag | Behaviour |
|---|---|
| (none) | List names of entries in current directory, space/newline separated |
| `-a` | Include hidden entries (names starting with `.`) |
| `-l` | Long format: permissions, links, owner, size, date, name |
| `-R` | Recursive listing using `walkdir` |
| `-la` / `-al` | Combined long format + hidden |
| `-lR` / `-Rl` | Combined long format + recursive |

Flags may be combined: `-la`, `-al`, `-lRa` etc. — standard single-dash
flag combination.

### Output Format

**Short format** (default, `-a`):
```
Cargo.lock
Cargo.toml
clank
clank-builtins
```
One entry per line. Sorted alphabetically. This matches macOS `ls` when
stdout is not a TTY (which is always the case in clank's pipe-based execution).

**Long format** (`-l`):
```
-rw-r--r--  1 user  staff   1234 Mar  7 12:00 Cargo.toml
drwxr-xr-x  5 user  staff    160 Mar  7 12:00 clank
```

On WASM (no Unix user/group APIs): owner and group columns are replaced with
`-` placeholders. This is honest per the README's "honest constraints" principle.

### Exit Codes

- `0` — success
- `1` — path does not exist or permission denied

### Sorting

Entries are sorted alphabetically by filename — case-sensitive, matching
standard `ls` behaviour on Linux. On macOS, `ls` defaults to case-insensitive
sort — the golden tests will be run against the local OS `ls`, so the sort
order will match whatever the OS produces.

---

## Unix-Specific Code Strategy

Permission bits (`-l` format) require `std::os::unix::fs::MetadataExt::mode()`.
This is only available on Unix. On WASM:

```rust
#[cfg(unix)]
fn format_permissions(meta: &Metadata) -> String {
    use std::os::unix::fs::MetadataExt;
    let mode = meta.mode();
    // format rwxrwxrwx string
}

#[cfg(not(unix))]
fn format_permissions(_meta: &Metadata) -> String {
    "----------".to_string()
}
```

Owner/group display similarly gated. This is the correct WASM-compatible
pattern per AGENTS.md.

---

## Golden Test Strategy

Golden tests derive expected output from the OS `ls` command for the same
input. Since `ls` output is directory-dependent, golden tests use a fixed
known directory (e.g. a temp directory set up by the test, or a stable
subdirectory in the repo).

Approach: create a fixed test fixture directory with known contents under
`clank/tests/golden/fixtures/ls-test-dir/`, committed to git. Run both
OS `ls` and clank's internal `ls` against it, assert they match.

---

## Conclusions

1. **Implementation uses `std::fs` for all filesystem operations** — fully
   WASM-compatible.
2. **`walkdir`** used for `-R` recursive listing — WASM-compatible, MIT/Unlicense.
3. **No new non-WASM dependencies** introduced.
4. **Unix-specific code** (permissions, owner, group) gated behind `#[cfg(unix)]`
   with WASM-safe fallbacks.
5. **Golden tests** use a committed fixture directory for reproducible output.
6. **Output format** matches OS `ls` when stdout is not a TTY — one entry per
   line, alphabetically sorted.
