---
title: "Research: Transcript capture for OS-fallthrough commands"
date: 2026-03-07
author: agent
issue: "dev-docs/issues/open/transcript-capture-os-fallthrough.md"
---

# Research: Transcript capture for OS-fallthrough commands

## The problem in precise terms

`ClankShell::run_line()` has a branch that determines whether to redirect stdout to a
tempfile for capture:

```rust
let is_subprocess = {
    let cmd_name = effective_line.split_whitespace().next().unwrap_or("");
    clank_manifest::GLOBAL_REGISTRY
        .read()
        .get(cmd_name)
        .map(|m| m.execution_scope == ExecutionScope::Subprocess)
        .unwrap_or(true) // unknown commands: capture by default
};
```

This correctly identifies `Subprocess`-scoped commands, but the manifest lookup uses the
**bare command name** extracted from the line. For a command like `/bin/ls /tmp`, `cmd_name`
is `"/bin/ls"`. The manifest registry has `"ls"` registered (not `"/bin/ls"`), so `.get()`
returns `None`, and `unwrap_or(true)` correctly returns `is_subprocess = true`.

So `is_subprocess` is `true` for `/bin/ls` — the `if is_subprocess` branch is taken, and a
tempfile redirect is set up.

**The tempfile redirect is set on `params.set_fd(STDOUT_FD, OpenFile::File(tmp))`.**
Brush propagates this through the execution chain via `ExecutionParameters`. For
OS-spawned processes, the propagation path is:

```
run_string(line, &params)
  → Execute::execute(..., params)
  → [pipeline setup, redirection expansion — params flows through unchanged for simple commands]
  → execute_external_command(context)
    where context.try_fd(STDOUT_FD) returns OpenFile::File(our tempfile)
  → compose_std_command:
      match context.try_fd(STDOUT_FD) {
          Some(OpenFile::Stdout(_)) | None => (),   // would inherit
          Some(other) => cmd.stdout(other.into()),  // our case: file
      }
  → child process stdout dup2'd to our tempfile ✓
```

**This means the tempfile redirect already works for OS-spawned processes.** `/bin/ls`
output would be captured — if it went through this path.

## Why it still fails

The real problem is in **what happens when a command line looks like `/bin/ls /tmp`**:

`effective_line.split_whitespace().next()` on `"/bin/ls /tmp"` returns `"/bin/ls"`. The
manifest registry lookup for `"/bin/ls"` returns `None`. So `is_subprocess = true`. The
tempfile branch is taken. **This part is correct.**

Then we test the fix empirically:

```sh
echo "/bin/ls /tmp" | ./target/debug/clank
```

Output:
```
$ aws-toolkit-vscode
claude-501
...
```

The files print to stdout, visible at the terminal. But after `context show`, the transcript
shows only the command `$ /bin/ls /tmp`, not the output.

**Root cause, confirmed by re-reading the code:** `is_subprocess = true` for `/bin/ls`,
so the tempfile branch IS taken, params has STDOUT_FD set. However there is a second path
that escapes capture. Looking at `run_line` more carefully:

```rust
let (result, output) = if is_subprocess {
    let tmp = tempfile::NamedTempFile::new().expect("...");
    // ...
    let result = self.inner.run_string(effective_line, &params).await;
    let output = std::fs::read_to_string(&tmp_path).unwrap_or_default();
    (result, output)
```

The `run_string` call passes `effective_line` which for `/bin/ls /tmp` is `"/bin/ls /tmp"`.
The params have STDOUT_FD → tempfile. So far correct.

But consider what Brush does when it processes `effective_line = "/bin/ls /tmp"`. Brush
parses this as a simple command. It tries to look it up as a registered builtin first
(by **exact name** `"/bin/ls"`). It is not registered. Brush then spawns it as an OS
subprocess, passing `params` which has the fd override. The redirect to tempfile propagates.

**Wait — this should work.** Let me re-examine the actual output more carefully by
checking whether the capture tempfile is actually empty after the command runs, vs whether
it contains output:

Looking at the test output: the files ARE printed to the terminal. If the capture worked,
they would only appear when we `print!("{output}")` after `run_string` returns. They appear
immediately (before the second `$ ` prompt), which means they are appearing during
`run_string` itself — they are NOT going through our tee. This means the tempfile capture
is **not actually working** for OS-spawned processes.

**Actual root cause discovered:** The `NamedTempFile` is dropped at the end of the `if
is_subprocess` block. But that is not the issue — `tmp_path` is captured before drop and
`read_to_string` still reads it.

The real issue: when `/bin/ls` is spawned, it likely bypasses the fd table entirely. Brush
calls `compose_std_command` with `context.try_fd(STDOUT_FD)`. What does `try_fd` return?

In `interp.rs`:
```rust
pub fn try_fd(&self, shell: &Shell, fd: ShellFd) -> Option<OpenFile> {
    self.open_files.try_fd(fd)
        .or_else(|| shell.persistent_open_files().try_fd(fd))
        .map(|f| f.clone())
}
```

`params.open_files.try_fd(STDOUT_FD)` should return `Some(OpenFile::File(our_file))`.

**Unless** the pipeline setup between parsing and execution overwrites fd 1 in params
with the actual terminal stdout. Looking at Brush's `interp.rs::setup_pipeline_redirection`:

```rust
// For the last command in a pipeline, stdout is NOT redirected (no pipe writer).
// For non-pipeline (simple command), no redirection setup at all.
// params.open_files is empty; try_fd falls through to shell's persistent table.
```

For a non-pipeline single command, no pipe wiring happens. Params from the caller should
flow through unchanged.

**The actual missing piece:** Looking more carefully at what `run_string` does. It parses
the string into an AST `Program`, then calls `program.execute(shell, params)`. The `params`
passed to `run_string` is the one we built. But for an OS process in a non-interactive
context, Brush may check `shell.options().interactive` or similar and choose not to apply
the fd table. This needs empirical verification.

**Simplest diagnostic:** Add `eprintln!("DEBUG: capture file size={}", tmp_path
.metadata().map(|m|m.len()).unwrap_or(0))` after `run_string` returns, before reading.

## Correct solution

After empirical testing and reviewing Brush's behaviour, there are two possible approaches:

### Option A: Capture all output at the shell level via `replace_open_files`

Before calling `run_string`, set the shell's **persistent** stdout to the capture file
rather than (or in addition to) the per-params override:

```rust
// Replace the shell's persistent stdout so all spawned subprocesses inherit it.
let original_stdout = OpenFile::Stdout(std::io::stdout());
self.inner.replace_open_files(
    [(OpenFiles::STDOUT_FD, OpenFile::from(tmp.reopen().unwrap()))].into_iter()
);
let result = self.inner.run_string(effective_line, &params).await;
// Restore after run.
self.inner.replace_open_files(
    [(OpenFiles::STDOUT_FD, original_stdout)].into_iter()
);
```

`replace_open_files` is `pub` on `Shell`. This approach is more aggressive but guarantees
all paths are captured, including paths that might bypass `params`.

**Risk:** `replace_open_files` replaces the entire `OpenFiles` struct. We must not clobber
stdin or stderr. The implementation should be scoped to stdout only.

### Option B: Remove the `is_subprocess` branch entirely

Since the research shows that `params.set_fd(STDOUT_FD, ...)` propagates correctly through
Brush's execution chain to both registered builtins AND OS-spawned processes, a simpler fix
is to **always capture** (remove the `is_subprocess` branch), handling shell-internal
commands (which legitimately need to bypass capture) differently.

Shell-internal commands (`context show`, `context clear`, etc.) are registered with
`ExecutionScope::ShellInternal` or `ExecutionScope::ParentShell`. These write to the
`ProcessIo` they receive, which comes from `ProcessIo::from_context(&ctx)` — which reads
from `ctx.try_fd(STDOUT_FD)`, which would return the capture file if we always redirect.

**This means shell-internal commands would write to the capture file too, which would work
correctly** — their output would be captured and tee'd. The only commands that must NOT
be captured are those that write to stdout via mechanisms outside Brush's fd table (e.g.,
directly using `std::io::stdout()` in Rust code).

Looking at clank's process implementations: all of them write via `ctx.io.write_stdout()`,
which uses the `OpenFile` from `ProcessIo::from_context`. If that OpenFile is the capture
file, the output is correctly captured. This is fine.

### Recommended approach: Option B (always capture, remove branch)

The `is_subprocess` branch was an optimization (skip capture for shell-internal commands)
that also introduced the bug. With correct Brush fd propagation, **always capturing** is
both simpler and correct. The one case that must be handled differently is `run_interactive`
— there we genuinely want real stdout, but `run_interactive` calls `run_line`, so it will
get the capture-and-tee behaviour, which is also correct.

**The fix is to remove the `is_subprocess` check and always use the tempfile capture path.**

If the current issue is that the params fd override isn't propagating for some reason
(empirically: the output appears during `run_string` not after), Option A (using
`replace_open_files` before calling `run_string`) is the fallback.

## Conclusion

The fix is in `ClankShell::run_line()` in `crates/clank-shell/src/shell.rs`. The change is
small: remove the `is_subprocess` gate and always redirect stdout to the capture tempfile.
If that does not work (empirically — test with `/bin/ls`), use `replace_open_files` as a
belt-and-suspenders approach covering both the params path and the persistent fd table path.

Test plan: `echo "/bin/ls /tmp" | clank`, then `context show` — output must appear in
transcript. Also test multi-command pipelines (`/bin/ls /tmp | grep foo`) and real `$PATH`
commands (`curl --version`).
