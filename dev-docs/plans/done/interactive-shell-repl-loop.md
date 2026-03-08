---
title: "Interactive Shell REPL Loop"
date: 2026-03-06
author: agent
issue: "dev-docs/issues/open/interactive-shell-repl-loop.md"
research: []
designs: []
---

# Interactive Shell REPL Loop

## Originating Issue

The shell does not operate as an interactive REPL — see `dev-docs/issues/open/interactive-shell-repl-loop.md`.

## Research Consulted

No formal research documents written. The relevant prior art is the standard Unix shell behavior for mode detection and the `brush-core` public API, both examined directly.

**Standard shell mode detection:** `bash`, `zsh`, `fish`, `dash`, and all POSIX-conforming shells use `isatty(STDIN_FILENO)` at startup to distinguish interactive from non-interactive mode. When stdin is a TTY, the shell enters an interactive read-eval-print loop. When stdin is a pipe or redirected file, the shell reads the entire input as a non-interactive script and exits. This is the industry-standard approach and is what the developer confirmed when asked.

**`brush-core` REPL primitives:**
- `Shell::new(options)` creates a persistent shell instance that retains state across multiple `run_string` calls.
- `Shell::run_string(cmd, &params)` returns `Result<ExecutionResult, Error>`. The `ExecutionResult` carries an `exit_code` and a `next_control_flow` field.
- `ExecutionResult::is_return_or_exit()` returns `true` when `next_control_flow` is `ExecutionControlFlow::ExitShell` — this is the correct signal to terminate the REPL loop.
- `Shell::compose_prompt()` composes a PS1-based prompt string using the shell's `$PS1` variable and brush's prompt expansion logic.

## Developer Feedback

Two design questions were posed:

1. **Mode detection:** Developer confirmed standard Unix TTY detection (`isatty` on stdin) is the correct approach, consistent with bash/fish/zsh behavior.
2. **Initial prompt string:** Developer chose `$` only (minimal, generic). PS1 customization is deferred as a separate concern.

## Approach

### Mode detection

`clank-shell/src/main.rs` gains a third execution mode:

| Condition | Mode | Behavior |
|---|---|---|
| Arguments provided | Argv mode | Join args, `run_string` once, exit (unchanged) |
| No args, stdin is **not** a TTY | Script mode | Read all stdin, `run_string` once, exit (unchanged) |
| No args, stdin is a TTY | **Interactive mode** | REPL loop (new) |

TTY detection uses `std::io::IsTerminal` (stabilized in Rust 1.70, available on the pinned stable toolchain). No new dependency required.

### REPL loop structure

The REPL loop lives in a new `clank_core::repl` module, exposed as a public `async fn run_interactive(shell: &mut Shell) -> Result<u8, Error>` function. Keeping the loop in `clank-core` rather than `clank-shell` ensures it is reachable by integration tests.

Loop behavior:
1. Print `$ ` prompt to stdout (no newline).
2. Read a line from stdin via `std::io::BufRead::read_line`. EOF (empty line after read) → break.
3. Strip the trailing newline. If the line is empty, loop (do not execute an empty command).
4. Call `shell.run_string(line, &params)`.
5. If `result.is_return_or_exit()` → break with the result's exit code.
6. Continue loop.

The function returns the last exit code seen (or `0` if no commands were executed before EOF).

### `CreateOptions` for interactive mode

Interactive mode sets `interactive: true` in `CreateOptions`. Non-interactive (script/argv) modes continue to use `interactive: false`. This matches how `brush-core` distinguishes the two modes internally.

### `clank-core` public API additions

```rust
pub async fn run_interactive(shell: &mut Shell) -> Result<u8, Error>;
pub fn interactive_options() -> CreateOptions;  // interactive: true variant
```

`run_interactive` is separate from `run` / `run_with_options` so the single-shot paths remain unchanged and the acceptance tests are unaffected.

### `clank-shell/src/main.rs` changes

The three-way dispatch is added:
```
if args not empty  → run_with_options(args.join(" "), default_options())
else if !stdin.is_terminal() → run_with_options(read_all_stdin(), default_options())
else → Shell::new(interactive_options()) → run_interactive(&mut shell)
```

### No new crate dependencies

`std::io::IsTerminal` covers TTY detection. `std::io::BufReader` + `read_line` covers line reading. No readline/libedit/rustyline required at this stage.

## Tasks

- [ ] Add `pub fn interactive_options() -> CreateOptions` to `clank-core/src/lib.rs` (sets `interactive: true`, otherwise same as `default_options`)
- [ ] Add `pub async fn run_interactive(shell: &mut Shell) -> Result<u8, Error>` to `clank-core/src/lib.rs` implementing the REPL loop
- [ ] Update `clank-shell/src/main.rs` to add TTY detection and dispatch to `run_interactive` when stdin is a terminal
- [ ] Add unit tests for `interactive_options()` fields
- [ ] Add integration test: `run_interactive` with a mock stdin containing two commands and an `exit 0` terminates correctly
- [ ] Add acceptance test case: pipe `echo hello\nexit 0\n` to clank and assert output contains `hello` (verifies stdin/script mode unaffected)
- [ ] Verify all existing acceptance tests still pass

## Acceptance Tests

- Piping `echo hello` to `clank` (stdin, non-TTY) → stdout contains `hello`, exit 0. Existing tests must continue to pass unchanged.
- Piping `echo first\necho second\nexit 3\n` to `clank` → stdout contains both lines, exit code is 3.
- Piping `echo hello\n` with no explicit `exit` → stdout contains `hello`, exit 0 (EOF terminates cleanly).
- No regression in `cases/exit_codes/basic.yaml`, `cases/scripting/pipelines.yaml`, or `cases/builtins/alias.yaml`.
