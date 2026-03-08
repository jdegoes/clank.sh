---
title: "No test infrastructure exists beyond basic unit tests"
date: 2026-03-06
author: agent
---

# No test infrastructure exists beyond basic unit tests

## Problem

The workspace has two inline `#[cfg(test)]` unit tests in `clank-shell/src/main.rs` and one ignored network test in `clank-http/src/lib.rs`. There is no integration test layer, no acceptance test layer, and no test case harness. The project cannot validate end-to-end observable behaviour, library API contracts, or scripting semantics in any systematic way.

## Impact

- Regressions in shell behaviour (exit codes, stdout/stderr, builtin semantics, pipelines) cannot be caught automatically.
- The library API boundary (`Shell::builder()`, option mapping, config loading) has no dedicated test coverage.
- There is no foundation for the oracle-based acceptance testing strategy described in the design — the approach that validates clank's scripting semantics against known-correct expected outputs, adapted from Brush's own test strategy.
- As builtins, the virtual filesystem, and authorization policy are implemented, there is nowhere to put behavioural regression tests that are both comprehensive and decoupled from implementation internals.

## Context

The clank.sh design calls for a single WebAssembly component with a synthetic process model. Testing that model requires a subprocess-level acceptance tier (spawning the compiled `clank` binary) in addition to in-process tiers. The three tiers are:

1. **Unit tests** — `#[cfg(test)]` inline in each crate. Test individual functions and small components. Already partially in place.
2. **Integration tests** — `tests/` directory in `clank-shell`. Compiled against `clank-shell` as a library. Use the Brush `Shell` API directly with no subprocess. Scope: `Shell::builder()` option mapping, `CreateOptions` field coverage, config loading.
3. **Acceptance tests** — a separate crate (`tests/acceptance/`) that spawns the compiled `clank` binary as a subprocess. Captures stdout, stderr, and exit code and asserts against them. Test cases are `.sh` files with structured comment annotations (`# expect-exit:`, `# expect-stdout:`, `# expect-stderr-empty:`).

## Out of Scope

This issue does not cover implementing any shell builtins, the virtual filesystem, authorization policy, MCP integration, or Golem durability features. It covers only the test infrastructure scaffolding — the harness, the crate structure, and an initial set of test cases sufficient to demonstrate all three tiers are functional.
