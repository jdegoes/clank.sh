---
title: "Interactive Shell REPL Loop"
date: 2026-03-06
author: agent
---

# Interactive Shell REPL Loop

## Overview

This document describes the interactive REPL loop as realized in the codebase. It supersedes the plan `dev-docs/plans/approved/interactive-shell-repl-loop.md` as the reference for future work in this area.

## Execution Mode Dispatch

`clank-shell/src/main.rs` dispatches across three modes in order:

1. **Argv mode** — one or more CLI arguments present: join with spaces, call `clank_core::run()` once, exit.
2. **Script mode** — no arguments, `stdin.is_terminal()` returns `false`: read all of stdin into a `String`, call `clank_core::run()` once, exit. This preserves the acceptance test harness behavior unchanged.
3. **Interactive mode** — no arguments, `stdin.is_terminal()` returns `true`: construct a shell with `clank_core::interactive_options()`, wrap stdin in a `BufReader`, call `clank_core::run_interactive()`.

TTY detection uses `std::io::IsTerminal`, stabilized in Rust 1.70. No external crate dependency.

## `clank-core` Public API

### `interactive_options() -> CreateOptions`

Returns `CreateOptions` with `interactive: true` and all other fields identical to `default_options()`. The `interactive` flag is passed through to `brush-core`'s shell initialization, which affects prompt behavior and error handling in the embedded interpreter.

### `run_interactive(shell: &mut Shell, input: impl BufRead, output: impl Write) -> Result<u8, Error>`

Runs a read-eval-print loop against a pre-constructed `Shell`. The `input` and `output` parameters are separated from the shell's internal file descriptors:

- `output` receives only the prompt string (`$ \n`-less prompt before each line read). It does not receive command output — command output goes to the shell's inherited stdout (file descriptor 1).
- `input` is the line source. `BufReader<Stdin>` in production; any `BufRead` implementor in tests.

This separation means unit tests can verify control-flow and exit-code semantics using `std::io::sink()` as the output and byte slices as input, without needing OS pipes or subprocess spawning.

Loop algorithm:

```
loop:
  write "$ " to output; flush
  read_line into buf
  if bytes_read == 0: break (EOF)
  strip trailing \r\n; skip if empty
  call shell.run_string(cmd, &params)
  record exit_code
  if result.is_return_or_exit(): break
return last_exit_code (0 if no commands executed)
```

### Exit signaling

`brush-core` signals `exit [n]` via `ExecutionControlFlow::ExitShell` on the returned `ExecutionResult`. `ExecutionResult::is_return_or_exit()` is the correct predicate to test. The exit code is taken from `result.exit_code`.

## Test Coverage

### Unit tests (`clank-core/src/lib.rs`)

| Test | Verifies |
|---|---|
| `interactive_options_is_interactive` | `interactive: true` |
| `interactive_options_skips_profile` | `no_profile: true` |
| `interactive_options_skips_rc` | `no_rc: true` |
| `interactive_options_disables_editing` | `no_editing: true` |
| `interactive_options_shell_name_is_clank` | `shell_name: Some("clank")` |
| `run_interactive_exits_zero_after_successful_commands` | EOF after `true; true` → exit 0 |
| `run_interactive_propagates_exit_code_from_exit_command` | `exit 7` → exit code 7 |
| `run_interactive_stops_after_exit_command` | `exit 3; exit 99` → exit code 3 (loop stops) |
| `run_interactive_returns_zero_on_eof_with_no_commands` | empty input → exit 0 |
| `run_interactive_returns_last_exit_code_on_eof` | `true; false` → exit nonzero |

### Acceptance tests (`clank-acceptance/cases/scripting/multi-command-stdin.yaml`)

| Case | Verifies |
|---|---|
| `multiple_commands_produce_all_output` | All command output appears; exit 0 |
| `exit_code_from_explicit_exit` | `exit 3` mid-script → exit code 3 |
| `eof_without_explicit_exit_returns_zero` | No `exit` → exit 0 |
| `last_command_determines_exit_on_eof` | `true; false` → exit 1 |

## Deviations from Plan

One deviation from the approved plan:

**`run_interactive` `output` parameter scope narrowed.** The plan stated the `output` parameter was for testing output capture. During implementation it became clear that `brush-core` writes command output directly to the shell's inherited file descriptor 1, not to any injected writer. The `output` parameter only receives prompt strings. This is noted in the function's doc comment. Output-content testing is correctly delegated to the acceptance test suite (subprocess capture) rather than unit tests.

## Known Limitations

- No readline / line editing (arrow keys, history recall). Deferred.
- No `PS1` / `PS2` customization. The prompt is hardcoded to `$ `.
- No `Ctrl-Z` / SIGTSTP support. Deferred per README non-goals.
