---
title: No formal test infrastructure — unit, integration, and system test layers are absent
date: 2026-03-07
author: agent
---

## Problem

The current test suite consists of four black-box integration tests in `clank/tests/repl.rs` that were written solely to verify the hello-world scaffold. They are unstructured, undocumented as to their layer or intent, and provide no foundation for the depth of testing required as the project grows.

clank.sh is production-facing tooling. Bugs in a shell — especially one that will be operated by an AI agent — can have serious consequences: data loss, silent failures, incorrect command execution, broken exit codes, corrupted state. The cost of a bug reaching production is high.

## Capability Gap

There is no defined test structure, no conventions for what belongs at each test layer, no shared test helpers, and no place to put unit tests for shell logic that will be extracted from `main.rs` as the codebase grows.

Specifically missing:

1. **Unit test layer** — no `#[cfg(test)]` modules, no internal test helpers, no way to test shell construction, REPL logic, or individual behaviours in isolation without spawning a process.
2. **Integration test layer** — the existing `clank/tests/repl.rs` is a flat file with no organisation by concern. There is no convention for how to group tests, name them, or share fixtures.
3. **System test layer** — no definition of what a system test is for this project, no location, no tooling.
4. **No shared test helpers** — common operations (build a shell, run a command, assert on stdout/stderr/exit code) are duplicated or missing entirely.
5. **No documentation** — no recorded decision on what belongs at each layer or how to add new tests.

## Why This Must Be Solved Now

Every subsequent issue will produce code that needs tests. Without a clear structure in place, tests will be written inconsistently, coverage will be uneven, and retrofitting structure later will be expensive and disruptive. The scaffolding must exist before significant feature work begins.

## Acceptance Condition

A developer can look at the test structure and immediately know:
- Where to add a unit test for a new function
- Where to add an integration test for a new shell behaviour
- Where to add a system test for an end-to-end scenario
- How to use shared helpers to avoid boilerplate

All existing tests continue to pass after the restructure.
