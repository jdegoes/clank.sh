---
title: "Script mode executes the entire stdin blob as one unit — transcript, output capture, and context commands do not work correctly"
date: 2026-03-07
author: agent
---

# Script mode executes the entire stdin blob as one unit

## Problem

`run_with_options` (script mode) reads the entire stdin content and passes it
to `shell.run_string(blob, &params)` as a single call. This produces three
visible problems:

**1. Transcript records one blob entry instead of per-command entries.**
The whole script is one `Command` entry. A script that types `echo hello`,
then `ls`, then `ask "what just happened?"` produces one transcript entry
containing all three lines rather than three separate entries with their
respective outputs. The README defines the transcript as "every command
typed, every output produced" — one blob entry per script run violates this.

**2. Output capture is not implemented for script mode.**
The plan for `transcript-entry-richness` attempted pipe-based stdout capture
in `run_with_options` but had to be reverted: injecting a pipe as stdout
captured all output into the transcript pipe, meaning nothing reached the
process's real stdout (the acceptance harness's pipe). A tee is not
possible with brush-core's `OpenFile` enum. The only viable path to per-command
output capture is the per-line model already used in `run_interactive`.

**3. `context show`, `context clear`, and `context trim` see an empty
transcript when invoked inside a script.**
Because the whole script is one `run_string` call and recording happens after
`run_string` returns, all commands inside the script execute before any
entries are recorded. `context show` always prints nothing; `context clear`
has no entries to clear; `context trim` has nothing to trim. The
`clank-acceptance` context test suite carries a large comment block explaining
and apologising for this limitation. It is not a limitation — it is a bug.

## Root Cause

`run_with_options` is modelled on running a script file: one call to
`run_string` with the whole content. The README does not distinguish between
interactive and non-interactive sessions for transcript purposes. The
transcript is a property of the session, not of the execution mode. Script
mode and interactive mode should produce identical transcript semantics for
the same sequence of commands.

## Research: Brush Oracle Testing

Brush's oracle test harness (`brush-test-harness`) feeds the entire `stdin`
YAML field to the shell subprocess as a single blob — the same model we
currently use. Brush does not use a line-by-line model for its oracle tests.
This is not a contradiction: brush's oracle tests validate scripting
language semantics (pipelines, variable expansion, arithmetic, etc.) against
bash, not transcript semantics. Transcript recording is a clank-specific
concern that sits above the shell interpreter.

The clank acceptance test harness mirrors brush's model correctly for
scripting tests. What needs to change is what `clank-core` does with the
script *before* passing it to brush-core — specifically, pre-parsing it into
individual statements and executing them one at a time through the same
record+execute+record loop that `run_interactive` already uses.

## Required Change

`run_with_options` must process the input script in the same per-statement
fashion as `run_interactive`:

1. Parse the script text into individual top-level statements using
   `brush-parser`.
2. Execute each statement via `shell.run_string(stmt, &params)`.
3. Record command text before execution, capture output, record output after
   execution — exactly as `run_interactive` does per line.

The result: script mode and interactive mode produce identical transcript
entries for the same sequence of commands. The acceptance test suite can then
assert on `context show` output after prior commands have run, on output
entries being present, and on `context clear` / `context trim` having visible
effect — the same assertions the integration tests in
`clank-core/tests/transcript.rs` already make.

## Brush Parser Integration

`brush-parser` is already a dependency of `clank-core`. Its public API
exposes `parse_script(source) -> Result<Program, ParseError>` where
`Program` is an AST of `CompleteCommand` nodes. Each `CompleteCommand`
corresponds to one top-level statement (a simple command, pipeline,
compound command, etc.). Reconstructing the source text of each
`CompleteCommand` for recording purposes may require either:

- Tracking source spans in the AST (check if `brush-parser` provides these), or
- Splitting the input on newlines and mapping statements back to lines.

The exact mechanism is a design decision for the plan.

## Acceptance Test Impact

Once script mode works line-by-line:
- The large `# ---------------------------------------------------------------------------` comment block in `context.yaml` explaining the script-mode limitation can be removed.
- Tests can assert `expect_stdout_contains: "command: echo hello"` after `echo hello; context show` because `echo hello` will be recorded before `context show` executes.
- Output capture tests become possible in the acceptance harness.
- The `show_output_is_not_re_recorded` test can be strengthened.

## Out of Scope

- Changing the brush oracle test harness or test case format.
- Any change to how `run_interactive` works — it already has correct semantics.
- Multi-line compound commands (e.g. `if/fi`, `for/done`) — these are single
  AST nodes and should be recorded as one entry, not split across their
  constituent lines.
