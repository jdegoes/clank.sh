---
title: "Golden test matrix — declarative shell behaviour test cases"
date: 2026-03-06
author: agent
issue: dev-docs/issues/open/golden-test-matrix.md
research: []
designs: []
---

## Summary

Replace the hand-written `clank-shell/tests/acceptance.rs` suite with a declarative golden test
matrix. Each case is a directory under `clank-shell/tests/golden/` containing a `stdin` file.
Expected `stdout` and `exit_code` are golden files managed by the `goldenfile` crate. Dynamic
test registration is provided by `test-r`'s `#[test_gen]` feature, giving each case a distinct
named entry in `cargo test` output.

## Design Decisions

### Test case layout

One directory per case under `clank-shell/tests/golden/`:

```
clank-shell/tests/golden/
  echo-hello/
    stdin       ← input fed verbatim to clank's stdin (hand-authored; never auto-updated)
    stdout      ← expected stdout (golden file; auto-updatable)
    exit_code   ← expected exit code as a plain integer string (golden file; auto-updatable)
```

`stdin` is always hand-authored — it is the test specification and must be committed explicitly.
`stdout` and `exit_code` are outputs managed by `goldenfile::Mint`. They are committed once
generated and updated via `UPDATE_GOLDENFILES=1 cargo test` when behaviour changes intentionally.

If `exit_code` is absent from a case directory, the test defaults to asserting exit code 0.

### Stdout matching

The actual stdout of the binary is written to the `goldenfile::Mint`. The `goldenfile` crate
performs the diff against the checked-in golden file and fails the test if they differ. No manual
stripping or normalisation is applied — the golden file contains exactly what the binary prints.

### Dynamic test registration — `test-r` `#[test_gen]`

`test-r` v3 (`vigoo/test-r`) supports runtime test generation via the `#[test_gen]` attribute.
A function annotated with `#[test_gen]` receives a `&mut TestProperties` and calls
`add_test(name, body)` for each case discovered on the filesystem. Each registered test appears
as a distinct named entry in `cargo test` output (e.g. `golden::echo_hello ... ok`).

This requires `harness = false` on the `golden` `[[test]]` target in `clank-shell/Cargo.toml`.
The harness is replaced by `test_r::main!()` in `golden.rs`. This applies only to the `golden`
target — other test targets are unaffected.

### Golden file management — `goldenfile`

`goldenfile` v1 (`calder/rust-goldenfile`, 2.2M downloads) is the standard crate for this
pattern. Usage:

```rust
let mut mint = goldenfile::Mint::new("tests/golden/echo-hello");
let mut stdout_file = mint.new_goldenfile("stdout").unwrap();
write!(stdout_file, "{}", actual_stdout).unwrap();
// mint drops here → diffs against checked-in golden file
```

Running `UPDATE_GOLDENFILES=1 cargo test` regenerates all golden files from actual output.
Workflow: run with the flag, review `git diff`, commit the changes.

### Replacing `acceptance.rs`

`acceptance.rs` is deleted. All existing acceptance test cases are replaced by golden cases
covering the same behaviours. `assert_cmd` and `predicates` are removed from `clank-shell`
dev-dependencies — the golden harness drives the binary directly via `std::process::Command`.

### New dependencies

| Crate | Version | Role |
|---|---|---|
| `test-r` | `3` | Dynamic test registration via `#[test_gen]` |
| `goldenfile` | `1` | Golden file diffing and `UPDATE_GOLDENFILES=1` auto-update |

Both added to `[workspace.dependencies]` and `clank-shell` `[dev-dependencies]`.

## Developer Feedback

Design decisions discussed with developer prior to writing this plan:

- **Test case location:** `clank-shell/tests/golden/` (co-located with binary acceptance tests).
- **Test case format:** one directory per case with `stdin`/`stdout`/`exit_code` files.
- **Test naming:** derive test name from directory name; each case is a distinct `cargo test` entry.
- **Test harness:** `test-r` with `#[test_gen]` for runtime discovery and registration.
- **Golden file tool:** `goldenfile` crate for diffing and auto-update workflow.
- **Stdout matching:** exact match (via `goldenfile` diff).
- **Fate of `acceptance.rs`:** replaced by golden cases; file deleted.

## Acceptance Tests

1. `cargo test` exits 0; each golden case appears as a distinct named test in output.
2. A new directory added under `tests/golden/` (with only a `stdin` file) is picked up
   automatically; `UPDATE_GOLDENFILES=1 cargo test` populates its `stdout` and `exit_code` golden
   files without requiring any Rust source changes.
3. Modifying a `stdout` golden file to contain wrong content causes only that case to fail.
4. `UPDATE_GOLDENFILES=1 cargo test` regenerates golden files to match actual output; `git diff`
   shows only the expected changes.

## Tasks

- [x] Create `dev-docs/issues/open/golden-test-matrix.md`
- [x] Create `dev-docs/plans/proposed/golden-test-matrix.md` (this file)
- [ ] Add `test-r = "3"` and `goldenfile = "1"` to `[workspace.dependencies]` in root `Cargo.toml`
- [ ] Add both to `[dev-dependencies]` in `clank-shell/Cargo.toml`
- [ ] Add `[[test]]` entry for `golden` with `harness = false` in `clank-shell/Cargo.toml`
- [ ] Delete `clank-shell/tests/acceptance.rs`
- [ ] Remove `assert_cmd` and `predicates` from `clank-shell` `[dev-dependencies]`
- [ ] Create `clank-shell/tests/golden.rs` — `test_r::main!()`, `#[test_gen]` walker, binary
      invocation via `std::process::Command`, `goldenfile::Mint` for stdout and exit_code
- [ ] Create `stdin` files for all 10 initial golden cases:
      `echo-hello`, `empty-input`, `variable-expansion`, `exit-zero`,
      `pipeline-basic`, `and-operator-success`, `and-operator-failure`,
      `or-operator-failure`, `last-exit-code`, `false-exit-code`
- [ ] Run `UPDATE_GOLDENFILES=1 cargo test` to populate initial `stdout` and `exit_code` golden files
- [ ] Verify `cargo test` exits 0 with all cases named individually
- [ ] Update `dev-docs/testing.md` to document the golden test layer, case format, and update workflow
