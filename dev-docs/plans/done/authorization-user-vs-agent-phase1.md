---
title: "Authorization: user vs. agent context — Phase 1 bypass with Phase 3 scaffold"
date: 2026-03-07
author: agent
issue: dev-docs/issues/open/authorization-bypass-for-user-context-phase1.md
research: []
designs: []
---

# Plan: Authorization — user vs. agent context

## Context

See `dev-docs/issues/open/authorization-context-user-vs-agent.md` for the architectural
issue and `dev-docs/issues/open/authorization-bypass-for-user-context-phase1.md` for the
immediate trigger.

In Phase 1, every command dispatched through `run_line` originates from a human typing at the
REPL. Applying `Confirm` and `SudoOnly` policies to user-typed commands is incorrect: the user
has already authorised the command by typing it. At the same time, the enforcement code and the
manifest policy values must be preserved intact for Phase 3, when an agent will issue commands
autonomously and the policies become meaningful safety mechanisms.

## Design

Introduce an `ExecutionContext` enum with two variants: `User` and `Agent`. Thread it through
the two enforcement points:

1. `run_line` in `shell.rs` — early `SudoOnly` deny
2. `dispatch_builtin` in `builtins.rs` — `Confirm` prompt

The existing `run_line(&mut self, line: &str) -> i32` signature is unchanged — it calls the
new internal implementation with `ExecutionContext::User`. A new public method
`run_line_as_agent(&mut self, line: &str) -> i32` calls the same implementation with
`ExecutionContext::Agent`.

The `ExecutionContext` is communicated from `run_line` to `dispatch_builtin` via a second
thread-local alongside `ACTIVE_SHELL_ID`, using the same pattern already in place for the
shell ID.

Enforcement logic at both points becomes:

```rust
if context == ExecutionContext::Agent {
    // enforce policy
}
// else: User context — execute without restriction
```

This is a conditional skip, not a deletion. Re-enabling enforcement for user context in a
future phase (e.g. if a "safe mode" is introduced) is a one-line change. The comment at each
bypass point cites both issues.

### Why a thread-local and not a ProcessContext field

`dispatch_builtin` is a bare fn-pointer callback (`CommandExecuteFunc`) required by Brush's
registration API. It cannot capture variables. The only way to pass per-invocation data into
it is via thread-locals, which is the same mechanism already used for `ACTIVE_SHELL_ID`. A
`ProcessContext` field would not reach `dispatch_builtin` — the context is constructed inside
`dispatch_builtin`, not passed into it.

---

## Tasks

- [ ] **T1 — Add `ExecutionContext` enum and `ACTIVE_EXECUTION_CONTEXT` thread-local**

  In `crates/clank-shell/src/builtins.rs`:
  - Add `pub enum ExecutionContext { User, Agent }` (derive `Clone`, `Copy`, `PartialEq`, `Eq`)
  - Add `thread_local! { pub static ACTIVE_EXECUTION_CONTEXT: Cell<ExecutionContext> = ... }`
    defaulting to `User`
  - Add `pub fn set_execution_context(ctx: ExecutionContext)` alongside `set_active_shell`
  - Export from `crates/clank-shell/src/lib.rs`

- [ ] **T2 — Thread `ExecutionContext` through `run_line` and the enforcement points**

  In `shell.rs`:
  - Extract the body of `run_line` into `run_line_with_context(&mut self, line: &str,
    context: ExecutionContext) -> i32`
  - `run_line` calls `run_line_with_context(line, ExecutionContext::User)`
  - Add `pub async fn run_line_as_agent(&mut self, line: &str) -> i32` that calls
    `run_line_with_context(line, ExecutionContext::Agent)`
  - In `run_line_with_context`: call `set_execution_context(context)` before `set_active_shell`
  - Wrap the `SudoOnly` early-deny block:
    ```rust
    // TODO(Phase 3 — agent context): remove this guard to enforce SudoOnly for user input
    // if a "safe mode" for user context is introduced.
    // See: dev-docs/issues/open/authorization-context-user-vs-agent.md
    if context == ExecutionContext::Agent {
        // existing SudoOnly deny logic
    }
    ```

  In `builtins.rs` `dispatch_builtin`:
  - Read `ACTIVE_EXECUTION_CONTEXT` at the top of the async block
  - Wrap the `Confirm` prompt block:
    ```rust
    // TODO(Phase 3 — agent context): same as above
    if execution_context == ExecutionContext::Agent {
        // existing Confirm prompt logic
    }
    ```

- [ ] **T3 — Rewrite `crates/clank-shell/tests/authorization.rs`**

  Replace the existing tests (which test Phase 3 agent-context behaviour incorrectly asserted
  against Phase 1 user-context input) with the 7 tests below. The existing file is deleted
  entirely and rewritten — the old tests are not correct for any phase as written, because
  they all use `run_line` (user context) to test agent-context behaviour.

  **User-context tests (Side A):**

  `test_user_confirm_command_executes_without_prompt`
  - Run `mkdir <tmpdir>/clank-auth-test-<uuid>` via `run_line`
  - Assert exit code is 0 (not 1 — the abort code)
  - Assert the directory was created on the real filesystem
  - This verifies the `Confirm` bypass works for user context

  `test_user_sudo_only_command_executes_without_sudo`
  - Run `rm /tmp/clank-auth-user-test-nonexistent` via `run_line`
  - Assert exit code is 1 (file not found — `rm` ran) not 5 (auth denied)
  - This verifies the `SudoOnly` bypass works for user context

  `test_user_sudo_prefix_still_strips_and_executes`
  - Run `sudo rm /tmp/clank-auth-sudo-test-nonexistent` via `run_line`
  - Assert exit code is 1 (rm ran) not 5 (denied)
  - This verifies that the `sudo` stripping mechanism is not broken by the bypass

  **Agent-context tests (Side B):**

  `test_agent_sudo_only_denied_without_sudo`
  - Run `rm /tmp/clank-auth-agent-test-nonexistent` via `run_line_as_agent`
  - Assert exit code is 5
  - This verifies `SudoOnly` enforcement is present and correct in agent context

  `test_agent_sudo_only_allowed_with_sudo`
  - Run `sudo rm /tmp/clank-auth-agent-sudo-test-nonexistent` via `run_line_as_agent`
  - Assert exit code is 1 (rm ran) not 5 (denied)
  - This verifies the `sudo` grant mechanism works in agent context

  `test_agent_confirm_command_prompts_and_aborts`
  - Run `mkdir <path>` via `run_line_as_agent` with stdin wired to `/dev/null`
    (so `read_line` gets EOF → empty answer → denial)
  - Assert exit code is 1 (aborted)
  - Assert the directory was NOT created
  - This verifies `Confirm` enforcement is present and correct in agent context

  `test_agent_sudo_state_cleared_after_command`
  - Run `sudo rm /tmp/clank-auth-agent-clear-test-nonexistent` via `run_line_as_agent`
  - Assert exit code is not 5 (ran)
  - Run `rm /tmp/clank-auth-agent-clear-test-nonexistent-2` via `run_line_as_agent`
  - Assert exit code is 5 (denied again — sudo state was cleared)
  - This verifies that one `sudo` grant does not persist to the next command in agent context

---

## Acceptance criteria

1. `mkdir demo` in the REPL executes without a confirmation prompt.
2. `rm file` in the REPL executes without requiring `sudo`.
3. `run_line_as_agent("rm ...")` exits 5 without `sudo`.
4. `run_line_as_agent("sudo rm ...")` executes (exits 1 on missing file).
5. `run_line_as_agent("mkdir ...")` with EOF stdin is aborted (exits 1, directory not created).
6. All 7 new authorization tests pass.
7. All existing tests continue to pass.
8. `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check` pass.
9. Both `TODO(Phase 3)` comments are present at the two enforcement bypass points.
10. Manifest policy values are unchanged (verified by existing manifest unit tests).

---

## Implementation notes

- The `run_line_as_agent` method will not be called from anywhere in Phase 1 production code.
  It exists solely as the Phase 3 entry point and as the test surface for agent-context
  behaviour. This is intentional.
- The `test_agent_confirm_command_prompts_and_aborts` test requires wiring stdin to a source
  that returns EOF. The `dispatch_builtin` `Confirm` path reads from `std::io::stdin()` via
  `spawn_blocking` — this cannot be easily injected. The simplest approach is to run the test
  with stdin redirected: `run_line_as_agent("mkdir <path> </dev/null")`. Brush will pass the
  redirect through to stdin before `dispatch_builtin` reads it.
  If that does not work due to how Brush handles redirects for registered builtins, an
  alternative is to add a `#[cfg(test)]` hook that allows injecting a fake stdin answer into
  the confirmation path, similar to how `ProcessIo` is injected into `Process::run`.
  Investigate at implementation time and choose the simpler approach.
- Do not change the `sudo` prefix stripping logic. It must remain in `run_line_with_context`
  for both `User` and `Agent` contexts — the difference is only in whether the resulting
  sudo state is checked for enforcement.
