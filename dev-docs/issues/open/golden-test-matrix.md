---
title: "No golden test matrix — shell behaviour cannot be specified declaratively"
date: 2026-03-06
author: agent
---

## Problem

The existing `clank-shell/tests/acceptance.rs` tests are hand-written Rust functions. Adding a
new case requires editing Rust source, and updating expected output after an intentional behaviour
change requires finding and editing string literals scattered across the file.

There is no way to express "given this stdin, expect this stdout and exit code" as a plain file
without touching test code. This makes the test suite expensive to grow and cumbersome to maintain.

## Desired Outcome

A declarative, file-based golden test matrix where:

- Each test case is a directory containing a `stdin` file specifying input.
- Expected `stdout` and `exit_code` are golden files managed by the `goldenfile` crate.
- Running `UPDATE_GOLDENFILES=1 cargo test` regenerates golden files from actual output, making
  expectation updates a review-and-commit workflow rather than a manual edit.
- Each case appears as a distinct named test in `cargo test` output.
- Adding a new case requires only creating a new directory — no Rust source changes.
