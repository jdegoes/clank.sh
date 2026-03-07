---
title: "cd/mkdir working directory fix — realized design"
date: 2026-03-07
author: agent
---

# Realized Design: cd/mkdir working directory fix

## Root cause confirmed

Brush's `cd` builtin calls `shell.set_working_dir()`, which updates Brush's internal
`working_dir: PathBuf` field and `PWD`/`OLDPWD` env vars, but does **not** call
`std::env::set_current_dir()`. The OS-level process cwd never changes.

The seven VFS-backed commands (`mkdir`, `rm`, `touch`, `cat`, `ls`, `grep`, `stat`)
previously called `std::path::Path::new(path)` and passed the result to `Vfs` methods, which
ultimately call `std::fs` operations. `std::fs` resolves relative paths against
`std::env::current_dir()` — the OS process cwd, which is never updated by `cd`. After any
`cd`, relative-path VFS operations silently operated on the wrong directory.

## What was built

### `ProcessContext.cwd: PathBuf`

A new field on `ProcessContext` populated in `dispatch_builtin` from
`ctx.shell.working_dir().to_path_buf()` — captured before the `async move` block consumes
`ctx`. This is the only correct source of truth for path resolution.

### `commands::resolve(cwd: &Path, path: &str) -> PathBuf`

A shared path resolution helper in `commands/mod.rs`:

```rust
pub(crate) fn resolve(cwd: &Path, path: &str) -> PathBuf {
    let p = Path::new(path);
    if p.is_absolute() { p.to_path_buf() } else { cwd.join(p) }
}
```

No canonicalisation — `..` and `.` are preserved, matching standard Unix tool behaviour.

### All 7 VFS commands updated

Every path argument in `MkdirProcess`, `RmProcess`, `TouchProcess`, `CatProcess`,
`LsProcess`, `GrepProcess`, and `StatProcess` is now passed through `resolve(&ctx.cwd, path)`
before reaching the Vfs. `std::path::Path::new(path)` no longer appears in any command
implementation.

### All test `ProcessContext` construction sites updated

All 11 test helper constructions of `ProcessContext` supply `cwd: PathBuf::from("/")` — a
safe default for tests that use `MockVfs` with absolute paths. Tests that need real path
resolution supply an appropriate `cwd`.

## Tests — four new tests would have caught the original bug

- **W5d** (unit): `resolve()` helper — relative, absolute, `..`, `.` cases
- **W5a**: `test_mkdir_after_cd_creates_in_new_cwd` — reproduces the exact user failure
- **W5b**: `test_ls_after_cd_shows_new_directory_contents` — read path coverage
- **W5c**: `test_chained_cd_relative_mkdir` — cumulative cwd across multiple `cd` calls

## Key decisions

- `cwd` is `PathBuf` not `&Path` — `ProcessContext` is consumed by `Process::run` across
  an async boundary; a borrowed path would require lifetime annotations through the async
  trait.
- No canonicalisation in `resolve()` — consistent with Unix tool behaviour and avoids
  symlink resolution surprises.
- `AskProcess` in `processes.rs` reads cwd from the transcript's working directory
  independently — not from `ProcessContext.cwd` — because `AskProcess` predates this change
  and reads directly from the shell. A future cleanup could unify these.
