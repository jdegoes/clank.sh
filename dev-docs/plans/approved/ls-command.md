---
title: Implement ls as Internal WASM-Compatible Command
date: 2026-03-07
author: agent
issue: dev-docs/issues/open/ls-command.md
research:
  - dev-docs/research/ls-command.md
designs:
  - dev-docs/designs/approved/internal-process-trait.md
---

## Summary

Implement `ls` as an internal `clank-builtins` command using `std::fs` and
`walkdir`. No OS process is spawned. All code compiles to `wasm32-wasip2`.
Unix-specific code (permissions, owner) is gated behind `#[cfg(unix)]` with
WASM-safe fallbacks. Behavioural equivalence with OS `ls` is enforced by
golden tests against a committed fixture directory.

## Developer Feedback

- All dependencies must be WASM-compatible — no `nix`, no `libc`.
- `uutils/coreutils` ruled out (non-optional `nix` dep).
- Implementation uses `std::fs` + `walkdir` only.
- **Code must be maximally readable.** No anonymous tuple types like
  `Vec<(String, String)>` where the meaning of each field is unclear.
  Use named structs or type aliases for every non-obvious type. A reader
  must be able to understand the code without needing to trace through
  multiple layers of type inference.

## Flags to Support

| Flag | Behaviour |
|---|---|
| (none) | List entry names, one per line, alphabetically sorted |
| `-a` | Include hidden entries (names starting with `.`) |
| `-l` | Long format: permissions, links, size, date, name |
| `-R` | Recursive listing via `walkdir` |
| Combinations | `-la`, `-al`, `-lR`, `-Rl`, `-lRa` etc. |

## Output Format

**Short format:** one filename per line, sorted alphabetically.

**Long format (`-l`):**
```
-rw-r--r--  1 user  staff   1234 Mar  7 12:00 Cargo.toml
drwxr-xr-x  5 user  staff    160 Mar  7 12:00 clank
```

On WASM (no Unix user/group APIs), owner and group columns emit `-`.
Permission bits are formatted with `#[cfg(unix)]` / `#[cfg(not(unix))]`.

## WASM Compatibility

| Concern | Solution |
|---|---|
| Filesystem traversal | `std::fs::read_dir` — supported on `wasm32-wasip2` |
| Recursive traversal | `walkdir` — no `nix`, no `libc`, WASM-compatible |
| Permission bits | `#[cfg(unix)]` `MetadataExt::mode()` / `#[cfg(not(unix))]` fallback |
| Owner/group | `#[cfg(unix)]` only / fallback to `-` on WASM |

## Golden Test Strategy

A fixture directory `clank/tests/golden/fixtures/ls-test-dir/` is committed
to git with known, stable contents. Golden YAML files assert that clank's
`ls` output matches what the OS `ls` would produce for the same input.

Fixture contents (simple, stable):
```
ls-test-dir/
├── a-file.txt
├── b-file.txt
├── .hidden-file
└── sub-dir/
    └── nested-file.txt
```

Golden tests cover: plain `ls`, `ls -a`, `ls -l`, `ls -la`, `ls -R`.

## New Dependency

`walkdir = "2"` added to `clank-builtins/Cargo.toml`. No other new deps.

## Acceptance Tests

1. `cargo test` passes — all existing 48 tests still green.
2. `cargo test --test golden` passes — golden tests for `ls` pass.
3. `ls`, `ls -a`, `ls -l`, `ls -la`, `ls -R` all execute without spawning an OS process.
4. `ls /nonexistent` exits with code 1 and prints an error to stderr.
5. `cargo clippy --all-targets -- -D warnings` passes.

## Tasks

- [ ] Add `walkdir = "2"` to `clank-builtins/Cargo.toml`
- [ ] Create `clank-builtins/src/ls.rs` implementing `LsCommand` with flags: `-a`, `-l`, `-R` and combinations
- [ ] Implement short format listing: `std::fs::read_dir`, sort alphabetically, filter hidden unless `-a`
- [ ] Implement long format (`-l`): size, modified time, file type indicator, name; gate permission bits and owner/group behind `#[cfg(unix)]`
- [ ] Implement recursive listing (`-R`): use `walkdir` with `min_depth(1)`
- [ ] Register `ls` in `clank_builtins::register()`
- [ ] Create `clank/tests/golden/fixtures/ls-test-dir/` with fixture files committed to git
- [ ] Write golden YAML tests: `ls-plain.yaml`, `ls-a.yaml`, `ls-l.yaml`, `ls-la.yaml`, `ls-R.yaml`
- [ ] Add integration tests to `clank/tests/builtins.rs` for `ls` error cases (nonexistent path)
- [ ] Verify all acceptance tests pass
