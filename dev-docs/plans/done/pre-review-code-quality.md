---
title: "Pre-review code quality remediation"
date: 2026-03-07
author: agent
issue: dev-docs/issues/open/pre-review-code-quality.md
research: []
designs: []
---

# Plan: Pre-review code quality remediation

## Context

A two-pass sweep of the codebase identified 15 issues — 7 from the first pass
(correctness/convention violations) and 8 from a systematic Rust idiom scan — that would raise
eyebrows in a peer review. All quality gates pass today (`cargo test`, `cargo clippy -D
warnings`, `cargo fmt --check`), so these are not compiler-visible problems. They are the kind
of issues that experienced Rust reviewers spot on reading the code.

The issues are addressed in priority order: architectural correctness first, then convention
violations, then idiom improvements.

---

## Acceptance criteria

- `cargo test --workspace` passes with zero failures after every task.
- `cargo clippy --all-targets -- -D warnings` passes.
- `cargo fmt --check` passes.
- Each issue from the issue doc is closed by a specific task below.
- No new `unwrap()`/`expect()` in library code introduced.

---

## Tasks

### Group A — Architectural / correctness

- [ ] **A1. Fix `context summarize` to use `AnthropicProvider` instead of hand-rolling HTTP**
  - `clank-shell/src/context_process.rs` currently builds the raw HTTP request manually,
    duplicating `AnthropicProvider`. Refactor `context summarize` to call the `AnthropicProvider`
    directly. The provider is already in scope via the `clank-ask` dependency.
  - The `ContextProcess` already receives an `Arc<dyn HttpClient>`; construct an
    `AnthropicProvider` from the config's api_key and the injected http client.
  - Remove the hand-rolled `serde_json::to_vec(...).expect(...)` call (fixes M2 automatically).
  - Update the existing `context summarize` tests in `clank-shell/tests/context.rs` to confirm
    behaviour is unchanged.

- [ ] **A2. Fix `model remove`/`model info` exit code from 1 to 2; add exit-code test**
  - `clank-ask/src/model_process.rs:12–13`: change exit code from `1` to `2`.
  - Rename `test_model_remove_stub_exits_1` → `test_model_remove_stub_exits_2` and add an
    assertion on `result.exit_code == 2`.

### Group B — Convention violations

- [ ] **B1. Eliminate `.unwrap()` in production `model list` output formatting**
  - `clank-ask/src/model_process.rs:57–59`: the `(true, true)` and `(true, false)` match arms
    call `.unwrap()` on `base_url` after already checking `has_url == true`.
  - Replace with `.unwrap_or("")` or refactor to extract `base_url` once before the match.

- [ ] **B2. Fix the misleading test name for `context summarize` graceful degradation**
  - `clank-shell/tests/context.rs:276`: rename
    `test_context_summarize_parse_failure_exits_1` →
    `test_context_summarize_wrong_shape_exits_0_gracefully` to accurately describe what it
    tests (exit 0 on wrong-shape 200 response, not exit 1 on parse failure).

- [ ] **B3. Add `tracing::warn!` for silent capture-file read failure**
  - `clank-shell/src/shell.rs:377`: replace bare `unwrap_or_default()` with a logged fallback:
    ```rust
    std::fs::read_to_string(&tmp_path).unwrap_or_else(|e| {
        tracing::warn!("failed to read subprocess capture file: {e}");
        String::new()
    })
    ```

- [ ] **B4. Remove `#[allow(dead_code)]` from scenario harness `desc` field**
  - `crates/clank/tests/scenario.rs:47`: the `desc` field exists for human readers of the
    YAML. Annotate it with `#[serde(default)]` only (already present) and remove the
    `#[allow(dead_code)]`. If the compiler still warns, use `let _ = &scenario.desc;` in the
    runner to suppress it, or simply mark the field with `_desc` if it is never programmatically
    read. Prefer removal of the suppress attribute over hiding it.

- [ ] **B5. Wrap `Confirm` stdin read in `spawn_blocking`**
  - `clank-shell/src/builtins.rs:342–344`: the blocking `stdin().lock().read_line(...)` inside
    the async `dispatch_builtin` closure is inconsistent with the REPL loop's use of
    `spawn_blocking`. Wrap the readline in `tokio::task::spawn_blocking` and `.await` it, matching
    the pattern in `run_interactive`. Note: `dispatch_builtin` returns a `BoxFuture`, so
    `spawn_blocking(...).await` is valid here.

### Group C — Rust idioms

- [ ] **C1. Replace `push_str(&format!(...))` with `write!(buf, ...)`**
  - Six production sites use `out.push_str(&format!(...))`, allocating a temporary `String`:
    - `model_process.rs:44,64`
    - `env_cmd.rs:15,17`
    - `ls.rs:67`
    - `grep.rs:150`
  - Replace each with `use std::fmt::Write as _; write!(out, ...).unwrap_or(())` or the
    equivalent. Writing to a `String` via `fmt::Write` is infallible, so the unwrap is
    semantically a no-op and can be written as `let _ = write!(out, ...)`.

- [ ] **C2. Eliminate `argv.clone()` in `prompt_user.rs` borrow workaround**
  - `prompt_user.rs:32,39` clones `ctx.argv` twice to work around a borrow conflict.
  - Restructure the flag parsing to extract all needed values from `ctx.argv` up front (before
    any `ctx.io` borrows), eliminating the need for the clones.

- [ ] **C3. Introduce `CommandOutput` struct for `run_model` / `run_ask` return type**
  - `(String, String, i32)` / `(String, Vec<u8>, i32)` anonymous tuple returns in
    `model_process.rs` and `ask_process.rs` are untyped and easy to misread.
  - Add a small named struct — or reuse/adapt `ProcessResult` — for these return values.
  - Candidate: a `pub struct CommandOutput { pub stdout: String, pub stderr: String, pub exit_code: i32 }`
    in `clank-ask/src/lib.rs`. Update `run_model` and its callers.
  - `run_ask` already returns `(String, Vec<u8>, i32)` (stderr is `Vec<u8>` for binary safety);
    evaluate whether a separate struct or a unified one is cleaner.

- [ ] **C4. Add `new()` constructors to command process structs; make `vfs` fields private**
  - `LsProcess`, `CatProcess`, `GrepProcess`, `StatProcess`, `MkdirProcess`, `RmProcess`,
    `TouchProcess` all expose `pub vfs: Arc<dyn Vfs>`.
  - Add a `pub fn new(vfs: Arc<dyn Vfs>) -> Self` constructor to each and make the field
    `pub(crate)` or private. Update all call sites in `shell.rs`.
  - `PsProcess` and `PromptUserProcess` similarly expose `pub shell_id: u64`; add constructors.
  - `EnvProcess` and `ExportProcess` are unit structs — no change needed.

- [ ] **C5. Avoid `buf.clone()` in REPL multi-line probe**
  - `shell.rs:474`: `self.inner.parse_string(buf.clone())` clones the accumulation buffer on
    every line during multi-line input. Check whether `parse_string` accepts `&str` or can be
    called with `buf.as_str()`. If the API requires ownership, restructure so the clone only
    occurs when `needs_more_input` returns true (i.e. the buffer is not yet complete), or
    accept the cost and document it.

---

## Implementation notes

- Tasks within each group are independent and can be done in any order, but complete Group A
  before moving to Groups B and C.
- After every task, run `cargo test --workspace` and `cargo clippy --all-targets -- -D
  warnings` before moving on.
- C3 (typed return struct) touches the most call sites and is the highest risk of introducing
  a regression; do it last within Group C and run the full scenario suite afterwards with
  `CLANK_UPDATE=1 cargo test --test scenario` to regenerate any fixtures if output changes.
- C4 (constructors) is purely mechanical but touches many files; do it as a single commit.
- Do not implement C5 if `parse_string` requires an owned `String` and the restructuring would
  make the code materially more complex — document the cost instead.

---

## Out of scope

- L2 (blocking Confirm readline → spawn_blocking) is included as B5 above but is deliberately
  the last item in Group B. It is the riskiest change because `dispatch_builtin` is a bare
  fn-pointer callback with specific lifetime constraints imposed by Brush. Verify carefully
  that `.await` inside the `BoxFuture` closure compiles and does not deadlock before marking
  done.
- No new user-visible features are introduced by this plan.
- No scenario fixture content changes are expected; run `CLANK_UPDATE=1` only if output
  changes are observed.
