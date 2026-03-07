---
title: "Plan: Fix transcript capture for OS-fallthrough commands"
date: 2026-03-07
author: agent
issue: "dev-docs/issues/open/transcript-capture-os-fallthrough.md"
research:
  - "dev-docs/research/transcript-capture-os-fallthrough.md"
---

# Plan: Fix transcript capture for OS-fallthrough commands

## Originating Issue

`dev-docs/issues/open/transcript-capture-os-fallthrough.md` — commands that fall through
to Brush's OS process-spawning path (full-path invocations like `/bin/ls`, or any
`$PATH`-resolved command not in the registry) produce output visible at the terminal but
never captured into the transcript.

## Research Summary

`dev-docs/research/transcript-capture-os-fallthrough.md` established the following:

1. `ClankShell::run_line()` branches on `is_subprocess` — a manifest lookup — to decide
   whether to redirect stdout to a capture tempfile. For `/bin/ls`, the lookup returns
   `None` (not in registry), so `unwrap_or(true)` gives `is_subprocess = true`, and
   the tempfile redirect IS set up.

2. Brush's `compose_std_command` uses `context.try_fd(STDOUT_FD)` to redirect child
   stdout. `try_fd` resolves: `params.open_files` first, then `shell.persistent_open_files()`.
   When `params` has the tempfile set, this should redirect child stdout to the file.

3. Empirically, the output still appears during `run_string` (not during our tee step),
   which means the capture is not working. The most likely explanation: for OS-spawned
   processes in certain Brush code paths, the params fd table is overwritten or not
   propagated correctly.

4. The reliable fix is to use **both** the per-params fd override (which we already do)
   AND `Shell::replace_open_files()` (which sets the shell's *persistent* stdout) before
   calling `run_string`, then restore it afterward. This covers all Brush execution paths
   regardless of which fd resolution chain is used.

5. As a simplification, the `is_subprocess` gate can be removed entirely. All commands —
   including shell-internals — write via `ctx.io.write_stdout()` which honors the
   captured fd. Shell-internal output being captured and tee'd is correct behaviour.

## Approach

### Change 1 — `ClankShell::run_line()`: always capture, use dual-path redirection

Remove the `is_subprocess` gate. Always redirect stdout to the capture tempfile via both:
- `params.set_fd(STDOUT_FD, OpenFile::File(our_file))` — existing per-params path
- `self.inner.replace_open_files(...)` — persistent shell table path (new)

Restore the persistent stdout after `run_string` returns.

```rust
pub async fn run_line(&mut self, line: &str) -> i32 {
    // ... command recording, proc snapshot, auth check unchanged ...

    // Always capture stdout — both for registered commands (which use params)
    // and OS-spawned processes (which may fall through to persistent open_files).
    let tmp = tempfile::NamedTempFile::new()
        .expect("failed to create capture temp file");
    let tmp_path = tmp.path().to_owned();
    let capture_file = tmp.reopen().expect("failed to reopen capture temp file");

    // Dual-path capture: set both the per-params fd and the persistent shell fd.
    let mut params = self.inner.default_exec_params();
    params.set_fd(
        brush_core::openfiles::OpenFiles::STDOUT_FD,
        brush_core::openfiles::OpenFile::from(
            capture_file.try_clone().expect("failed to clone capture file"),
        ),
    );
    self.inner.replace_open_files(
        [(
            brush_core::openfiles::OpenFiles::STDOUT_FD,
            brush_core::openfiles::OpenFile::from(capture_file),
        )]
        .into_iter(),
    );

    let result = self.inner.run_string(effective_line, &params).await;

    // Restore the shell's persistent stdout to the real terminal.
    self.inner.replace_open_files(
        [(
            brush_core::openfiles::OpenFiles::STDOUT_FD,
            brush_core::openfiles::OpenFile::Stdout(std::io::stdout()),
        )]
        .into_iter(),
    );

    let output = std::fs::read_to_string(&tmp_path).unwrap_or_default();

    // ... tee + transcript append unchanged ...
}
```

### Change 2 — Remove the `is_subprocess` determination block

The 10-line block that computes `is_subprocess` is no longer needed. Remove it.

### Change 3 — Handle `replace_open_files` restoration on early return

The `run_line` function has one early return path (SudoOnly denial, exit 5). This returns
before `run_string` is called, so no `replace_open_files` has happened yet. No restoration
is needed there. The code path is safe as-is.

---

## Acceptance tests

### Level 2 — Crate integration (new file: `crates/clank-shell/tests/transcript_os_commands.rs`)

```
test_os_fullpath_output_captured_in_transcript
  - Run `shell.run_line("/bin/echo hello")`.
  - Assert transcript contains an Output entry with "hello".

test_os_path_command_output_captured_in_transcript
  - Run `shell.run_line("echo hello")` (resolved via $PATH, not dispatch table).
  - Assert transcript output entry contains "hello".

test_os_pipeline_output_captured_in_transcript
  - Run `shell.run_line("echo 'alpha\nbeta\ngamma' | grep beta")`.
  - Assert transcript output entry contains "beta".

test_shell_internal_output_still_works_after_change
  - Add a known entry to transcript. Run `context show`.
  - Assert the known entry appears in stdout (shell-internal output not broken).

test_capture_does_not_bleed_across_commands
  - Run `/bin/echo first`, then `/bin/echo second`.
  - Assert first transcript Output entry is "first", second is "second".
```

### Level 3 — Scenario fixture

New fixture `tests/scenarios/shell_basics/os_fallthrough_capture.yaml`:

```yaml
desc: "OS-fallthrough command output is captured into the transcript"
stdin: |
  /bin/echo "hello from os"
  context show
stdout: |
  $ hello from os
  $ [Command]  /bin/echo "hello from os"
  [Output]   hello from os
  [Command]  context show
  $ 
```

(The exact format of `context show` output will be confirmed on first `CLANK_UPDATE=1` run.)

---

## Tasks

- [ ] **T1** Remove the `is_subprocess` block from `run_line()`; implement dual-path capture with `replace_open_files` + `set_fd`; ensure persistent stdout is restored after `run_string`
- [ ] **T2** Add Level 2 crate integration tests in `crates/clank-shell/tests/transcript_os_commands.rs`
- [ ] **T3** Add Level 3 scenario fixture `tests/scenarios/shell_basics/os_fallthrough_capture.yaml`; run `CLANK_UPDATE=1` to capture expected output
- [ ] **QG** `cargo test --workspace`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check` all pass; empirically verify `/bin/ls /tmp | grep tmp` output appears in `context show`
