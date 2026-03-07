---
title: Test Infrastructure Conventions for Rust CLI and Shell Projects
date: 2026-03-07
author: agent
---

## Purpose

Determine the right structure, tooling, and conventions for unit, integration, and system tests in a Rust project that is a shell interpreter binary. The goal is a layered, comprehensive test infrastructure that can grow with the project and prevent bugs in production-facing tooling.

---

## Rust's Native Test Layers

Rust provides two first-class test locations, each with different capabilities:

### 1. Unit Tests — `src/**/*.rs` with `#[cfg(test)]`

- Live in the same file as the code they test, inside a `mod tests { ... }` block gated by `#[cfg(test)]`.
- Can access **private** functions and types — essential for testing internal logic.
- Fast: no binary spawn, no process overhead.
- Best for: pure logic, state machines, parsing, individual function correctness.

```rust
// src/repl.rs
pub fn strip_comment(line: &str) -> &str { ... }

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn strips_inline_comment() {
        assert_eq!(strip_comment("echo hi # comment"), "echo hi ");
    }
}
```

### 2. Integration Tests — `tests/` directory

- Each `.rs` file in `tests/` is compiled as a **separate crate**.
- Can only access **public** API.
- Cargo automatically discovers and runs them with `cargo test`.
- Best for: testing that public APIs compose correctly, testing shell behaviours from the outside.

For a binary crate (which `clank` is), integration tests typically use `assert_cmd` to spawn the binary and assert on stdout/stderr/exit code.

For a library crate, integration tests call public functions directly.

**Shared helpers** go in `tests/common/mod.rs` (or `tests/support/mod.rs`). This file is not treated as a test file itself — it is a module imported by the actual test files.

```
clank/tests/
├── common/
│   └── mod.rs      ← shared helpers (not a test file)
├── repl.rs         ← REPL behaviour tests
├── builtins.rs     ← builtin command tests
└── exit_codes.rs   ← exit code tests
```

---

## System Tests

System tests verify end-to-end scenarios across the full stack. For a shell binary, this means:
- Multi-command sequences with state carried between them
- Pipe chains
- Variable assignment and expansion across multiple lines
- Error recovery scenarios

In Rust, system tests are a subset of integration tests — they live in `tests/` and use `assert_cmd`. The distinction is one of intent and complexity, not location:
- **Integration test**: verifies one behaviour in isolation (e.g. `echo` works)
- **System test**: verifies a realistic multi-step scenario (e.g. assign variable, use it in a pipe, check exit code)

A separate subdirectory `tests/system/` or a naming convention (`tests/system_*.rs`) makes the distinction explicit.

---

## The Library/Binary Split — Key Architectural Decision

The current `clank` crate is a **pure binary crate** (`src/main.rs` only). This means:

- Unit tests inside `main.rs` can only test things in `main.rs`.
- There is no library to call from integration tests — only the binary can be tested via process spawn.

**The standard Rust solution** is the library/binary split:

```
clank/
├── src/
│   ├── lib.rs      ← library crate: all testable logic lives here
│   └── main.rs     ← thin binary: calls lib
└── tests/
    └── ...         ← integration tests call lib directly OR spawn binary
```

With `lib.rs` in place:
- Unit tests cover internal logic directly (no process spawn needed)
- Integration tests can call `clank::build_shell()` or `clank::run_command()` without spawning a process — faster and more precise
- The binary remains a thin wrapper that calls into the library

This split is the standard approach for any non-trivial Rust CLI and is the correct foundation for comprehensive testing.

---

## Tooling

| Crate | Role | Already in project |
|---|---|---|
| `assert_cmd` | Spawn binary, assert on stdout/stderr/exit code | Yes (dev-dep) |
| `predicates` | Composable string/boolean matchers for assert_cmd | Yes (dev-dep) |
| `tokio::test` | `#[tokio::test]` macro for async unit/integration tests | Via tokio (already dep) |
| `tempfile` | Create temporary directories for filesystem tests | No — add when needed |
| `assert_fs` | Filesystem fixture assertions | No — add when needed |

No new runtime test tooling is needed beyond what is already present. The `tokio` dep already provides `#[tokio::test]`.

---

## Naming Conventions

Clear, consistent test names are essential for a growing test suite:

- **Unit tests**: `fn <what>_<condition>_<expected_result>()`  
  e.g. `fn run_string_empty_input_returns_ok()`
- **Integration tests**: `fn <command>_<scenario>()`  
  e.g. `fn echo_multiword_prints_all_words()`
- **System tests**: `fn scenario_<descriptive_name>()`  
  e.g. `fn scenario_variable_assignment_and_expansion()`

---

## What the Existing Tests Are

The four tests in `clank/tests/repl.rs` are integration tests (they spawn the binary). They test:
- `echo hello` → stdout contains `hello`
- `ls` → non-empty stdout
- `true; echo $?` → stdout contains `0`
- `false; echo $?` → stdout contains `1`

These are valid integration tests. They should be moved to a better-organised location (`tests/repl.rs` → `tests/integration/repl.rs` or regrouped by concern) and supplemented with unit tests once `lib.rs` exists.

---

## Conclusions

1. **Split `main.rs` into `lib.rs` + `main.rs`** — this is the prerequisite for unit testing internal logic.
2. **Unit tests live in `#[cfg(test)]` blocks in `src/`** — they test private logic directly.
3. **Integration tests live in `clank/tests/`**, organised into files by concern (e.g. `repl.rs`, `builtins.rs`, `exit_codes.rs`).
4. **System tests live in `clank/tests/system/`**, using multi-step scenarios.
5. **Shared helpers live in `clank/tests/common/mod.rs`** — a `clank()` builder fn, helpers for asserting stdout/stderr, etc.
6. **Existing tests are migrated** into the new structure — nothing is lost, only reorganised.
7. **`#[tokio::test]`** is used for async unit/integration tests that call shell logic directly.
8. No new crates are needed right now beyond what is already present.
