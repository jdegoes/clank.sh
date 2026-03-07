---
title: "cd fails after mkdir when Brush working directory diverges from process cwd"
date: 2026-03-07
author: agent
---

# cd fails after mkdir when Brush working directory diverges from process cwd

## Observed behaviour

Running `mkdir demo && cd demo` (or sequentially `mkdir demo` then `cd demo`) produces:

```
error: cd: i/o error: No such file or directory (os error 2)
```

`mkdir demo` succeeds (the directory is created on the real filesystem), but the subsequent
`cd demo` fails to find it.

## Root cause

`MkdirProcess` creates directories via `RealFs::create_dir`, which calls `std::fs::create_dir`
with a path resolved relative to `std::env::current_dir()` — the OS-level process working
directory.

`cd` is a `ParentShell` builtin handled by Brush, which resolves the path relative to its
own internally-tracked working directory. Brush initialises this from the environment at shell
startup, but it is a separate copy — not a live view of `std::env::current_dir()`.

When a relative path like `demo` is passed to `mkdir`, the directory is created relative to
the OS cwd. When `cd demo` is then run, Brush resolves `demo` relative to its internal cwd.
If these are the same directory (the common case), it works. However under certain startup
conditions they can diverge, causing `cd` to look in the wrong place.

## Fix required

Investigate whether `MkdirProcess` should resolve relative paths against Brush's working
directory rather than the OS cwd, or whether the shell initialisation should ensure Brush's
internal cwd is always kept in sync with `std::env::current_dir()`. The latter is the more
robust fix as it would affect all commands consistently.
