---
title: No golden tests — shell output regressions are undetected
date: 2026-03-07
author: agent
---

## Problem

The current system tests assert on broad behavioural properties (e.g. "stdout contains X") but do not lock down the exact output of shell commands. As the shell evolves, subtle changes in output formatting, whitespace, error messages, or builtin behaviour can slip through unnoticed.

Golden tests solve this by recording the exact stdout and stderr produced by a given input, and asserting that future runs produce byte-identical output. Any deviation is a regression until explicitly acknowledged and the golden file updated.

## Capability Gap

There is no mechanism to:
- Specify a shell input and its expected exact output in a declarative, reviewable format
- Detect regressions in exact output across changes to the shell or its dependencies
- Clearly communicate "this is the intended output of this command" as a reviewable artefact

## Design Constraints

- Golden tests are a subset of system tests, discovered and run as part of `cargo test`.
- Each test case is a plain YAML file specifying: shell input (one or more lines) and expected stdout or stderr (or both).
- The test runner is written once in Rust. It discovers all YAML files automatically and runs each one as a test case against the clank shell.
- No Rust required to add a new test. Anyone who can write YAML can add a behavioural test.
- Updating golden output must be an explicit, deliberate act — not something that happens automatically.

## Acceptance Condition

A developer adds a new `.yaml` file, runs `cargo test`, and the test runner discovers and executes it automatically. If the output does not match, the test fails with a clear diff. No Rust code needs to be touched.
