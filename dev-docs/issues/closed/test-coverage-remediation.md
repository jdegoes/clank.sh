---
title: "Test coverage violations against AGENTS.md mandatory coverage rules"
date: 2026-03-06
author: agent
---

# Test coverage violations against AGENTS.md mandatory coverage rules

## Problem

A systematic coverage audit has identified numerous gaps where existing code does not satisfy
the mandatory testing requirements stated in AGENTS.md. Several of these are direct violations
of the "before marking a task complete" checklist and the mandatory coverage table. They
represent incomplete prior work, not a backlog of optional improvements.

## AGENTS.md rules violated

The following mandatory rules are not met by current code:

1. **"Every new non-trivial behaviour ships with tests."** — Seven `Process` implementations
   in `clank-shell/src/commands/` (`CatProcess`, `GrepProcess`, `LsProcess`, `MkdirProcess`,
   `RmProcess`, `TouchProcess`, `StatProcess`) have zero tests at any level. `AskProcess` and
   `ModelProcess` in `crates/clank/src/processes.rs` also have zero tests.

2. **"`HttpError`: conversion from `reqwest::Error`, display strings — Unit (Level 1)"** —
   Explicitly named in AGENTS.md as mandatory. `clank-http` has zero tests. The `From<reqwest::Error>`
   impl and all four `HttpError` display strings are untested.

3. **"A new `Process` implementation — Unit (Level 1) + Crate integration (Level 2)"** — All
   nine `Process` implementations listed above violate this rule.

4. **"A new builtin command — Crate integration (Level 2) via `run_line()`"** — `ContextProcess`
   has integration tests, but the `summarize` subcommand is structurally untestable because
   `clank_http_config()` reads config from disk rather than using the injected `http` field.
   This is a design defect that prevents compliance.

5. **"Error type conversions and display — Unit (Level 1)"** — `VfsError` display strings,
   `HttpError` display strings, and `HttpError::from(reqwest::Error)` are all untested.

6. **"Exit code contract for a command — Crate integration (Level 2)"** — The authorization
   enforcement exit code (5 for `SudoOnly` without sudo) is never tested. Exit codes for all
   filesystem commands are untested. The exit code truncation bug (`as u8` in `dispatch_builtin`)
   has no test.

7. **"Always assert on both stdout and stderr explicitly."** — Multiple system tests only
   check one stream or neither.

8. **Duplicate test:** Scenario fixtures `ask_stub.yaml` and `ask_no_config.yaml` are
   byte-for-byte identical. One adds zero coverage.

9. **Weak test assertions:** `test_model_list_no_config` accepts two contradictory outcomes
   with `||`. `test_context_show_empty` in `ask.rs` would pass on any string containing
   `"context"`. `test_context_summarize_calls_model` and `test_context_summarize_exits_0_or_1`
   both acknowledge they cannot test the actual behaviour.

## Capability Gap

- `clank-http`: zero tests despite explicit AGENTS.md callout
- `clank-vfs`: zero tests for `MockVfs`, `ProcHandler`, `LayeredVfs`, `VfsError`
- `clank-shell/commands/`: seven commands with zero tests at any level
- `context summarize`: structurally untestable due to design defect in `clank_http_config()`
- `AskProcess::run()` and `ModelProcess::run()`: zero tests
- Authorization enforcement (SudoOnly deny, exit 5): zero tests
- `SecretsRegistry::remove()` and `snapshot()`: never verified
- `process_table::kill()` and `set_abort_handle()`: never tested
- `dispatch_builtin` exit code truncation (`as u8`): no test
- `clank-manifest` authorization policies: never asserted

## Out of Scope

This issue covers test gaps in existing code only. It does not cover new features,
Phase 3/4 functionality, or the scenario test harness (addressed separately).
