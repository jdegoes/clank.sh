---
title: "Fix working directory divergence between Brush and OS process cwd"
date: 2026-03-07
author: agent
issue: dev-docs/issues/open/cd-fails-after-mkdir.md
research: []
designs: []
---

# Plan: Fix working directory divergence between Brush and OS process cwd

## Root cause

Brush's `cd` builtin calls `shell.set_working_dir()`, which updates Brush's internal
`working_dir: PathBuf` field and the `PWD` / `OLDPWD` environment variables. It does **not**
call `std::env::set_current_dir()` — the OS-level process working directory never changes.

The VFS-backed commands (`mkdir`, `rm`, `touch`, `cat`, `ls`, `grep`, `stat`) receive relative
paths and resolve them via `std::fs` / `RealFs`, which always resolves against
`std::env::current_dir()` — the OS process cwd. After a `cd`, the two working directories
diverge and any relative-path VFS operation operates on the wrong directory.

**Reproduced:** `cd /tmp && mkdir clank-test && cd clank-test` fails with `No such file or
directory` because `mkdir clank-test` creates `{launch_dir}/clank-test` while `cd clank-test`
looks for `/tmp/clank-test`.

## Fix

### Task W1 — Add `cwd: PathBuf` to `ProcessContext`

`ProcessContext` in `crates/clank-shell/src/process.rs` gains a new field:

```rust
/// The shell's current working directory at the time of dispatch, as tracked
/// by Brush. All relative paths in VFS commands must be resolved against this.
pub cwd: PathBuf,
```

This is the single source of truth for path resolution in clank commands. It is populated
in `dispatch_builtin` from `ctx.shell.working_dir()` — the Brush shell's internal working
directory, which is correctly updated by `cd`.

### Task W2 — Populate `cwd` in `dispatch_builtin`

In `crates/clank-shell/src/builtins.rs`, update the `ProcessContext` construction in
`dispatch_builtin` to include `cwd`:

```rust
let result = process
    .run(ProcessContext {
        argv,
        env: current_env_snapshot(),
        io,
        pid,
        cwd: ctx.shell.working_dir().to_path_buf(),
    })
    .await;
```

### Task W3 — Update all VFS command implementations to resolve paths against `ctx.cwd`

The following commands accept path arguments that must be resolved against `ctx.cwd` when
the path is relative:

- `MkdirProcess` (`mkdir.rs`) — `create_dir` / `create_dir_all`
- `RmProcess` (`rm.rs`) — `remove_file` / `remove_dir_all`
- `TouchProcess` (`touch.rs`) — `write_file`
- `CatProcess` (`cat.rs`) — `read_file`
- `LsProcess` (`ls.rs`) — `read_dir`
- `GrepProcess` (`grep.rs`) — `read_file` / `read_dir` (recursive)
- `StatProcess` (`stat_cmd.rs`) — `metadata`

The pattern for each:

```rust
fn resolve(cwd: &Path, p: &str) -> PathBuf {
    let path = Path::new(p);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        cwd.join(path)
    }
}
```

This helper can be a private free function in a shared location (e.g. `commands/mod.rs` or
inline in each command). Each command calls `resolve(&ctx.cwd, path_arg)` before passing the
path to the VFS.

### Task W4 — Update all `ProcessContext` construction sites in tests

All tests that construct `ProcessContext` directly must supply a `cwd`. Use
`PathBuf::from("/")` as the default for tests that do not care about path resolution, or a
`tempdir().path().to_path_buf()` for tests that do. Update:

- All `#[cfg(test)]` helpers in each command file
- `crates/clank-shell/tests/` test helpers
- `crates/clank/tests/processes.rs` `make_ctx` helper

### Task W5 — Add tests that would have caught this issue before a user hit it

These tests are specifically designed to catch the cwd divergence class of bug. They must
all use `RealFs` (not `MockVfs`) and must follow a `cd` with a VFS command — the combination
that was never tested before.

Add to a new file `crates/clank-shell/tests/working_directory.rs`:

**W5a** — `test_mkdir_after_cd_creates_in_new_cwd`

The exact scenario the user hit. Would have caught the bug immediately.

- Run `cd /tmp` then `mkdir clank-wd-test-<uuid>` via `run_line`
- Assert the directory was created under `/tmp/`, not the process launch directory
- Assert exit code is 0
- Clean up

**W5b** — `test_ls_after_cd_shows_new_directory_contents`

Catches the same divergence on the read path. Would have caught it for `ls`, `cat`, `grep`.

- Create a temp dir containing a file with a known sentinel name
- Run `cd <tempdir>` then `ls` via `run_line`
- Assert the sentinel filename appears in transcript output
- Assert the current working directory shown by `pwd` is the temp dir

**W5c** — `test_chained_cd_relative_mkdir`

Catches that cwd is cumulative across multiple `cd` invocations — the divergence compounds.

- Run `cd /tmp`, then `mkdir clank-wd-chain-<uuid>`, then `cd clank-wd-chain-<uuid>`,
  then `mkdir inner` via separate `run_line` calls
- Assert `/tmp/clank-wd-chain-<uuid>/inner` exists on the real filesystem
- Clean up

**W5d** — Unit test for the `resolve()` path helper (Level 1, inline in the module where
the helper is defined)

Catches any mistake in the resolution logic itself, independently of the shell machinery.

- `resolve(Path::new("/tmp"), "demo")` → `/tmp/demo`
- `resolve(Path::new("/tmp"), "/absolute/path")` → `/absolute/path`
- `resolve(Path::new("/tmp"), "../sibling")` → `/tmp/../sibling` (not canonicalised — correct)
- `resolve(Path::new("/a/b/c"), ".")` → `/a/b/c/.`

---

## Acceptance criteria

1. `cd /tmp && mkdir clank-test && cd clank-test` succeeds — no "No such file or directory".
2. `mkdir demo` followed by `cd demo` on separate lines succeeds.
3. Relative paths in `ls`, `cat`, `grep`, `stat`, `rm`, `touch` all resolve against the
   Brush working directory after `cd`.
4. Absolute paths are unaffected.
5. Tests W5a, W5b, W5c, and W5d all pass.
6. All existing tests continue to pass.
7. `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check` pass.

---

## Implementation notes

- The `cwd` field on `ProcessContext` is `PathBuf`, not `&Path` — the context is consumed
  by `Process::run` which takes ownership, so a borrowed path would require lifetime
  annotations across the async trait boundary. `PathBuf` (one allocation per dispatch) is
  the correct type.
- `AskProcess` already reads cwd independently from Brush's working directory via
  `ctx.shell.working_dir()` in the `clank` crate's `processes.rs`. After this change,
  `ProcessContext.cwd` carries the same value — `AskProcess` should be updated to read from
  `ctx.cwd` instead of re-reading from the shell for consistency.
- `EnvProcess` and `ExportProcess` do not handle paths — no change needed.
- `PsProcess` and `PromptUserProcess` do not handle paths — no change needed.
- The `make_ctx` helper in `crates/clank/tests/processes.rs` must be updated; all tests
  using it will recompile automatically.
