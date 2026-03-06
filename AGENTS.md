# AGENTS.md

## Project

clank.sh is an AI-native shell targeting `wasm32-wasip2` and native Rust. See `README.md` for full design documentation.

## Build & Test

### Prerequisites

A C compiler must be on `$PATH` for linking. On NixOS, enter a dev shell with `nix-shell -p gcc`
or equivalent before running Cargo commands.

### Building

```bash
cargo build               # build all crates (native)
cargo run --bin clank     # build and run the shell binary
cargo check               # check without producing artifacts (faster)
cargo build --release     # release binary → target/release/clank
```

**Dependency notes:**
- `reqwest` uses `rustls-tls` — no OpenSSL system dependency required.
- `brush-core` and its dependencies are native-only; do not attempt `cargo build --target wasm32-wasip2`.

### Definition of done

A task is complete when ALL of the following pass without errors or warnings:

1. `cargo build` exits 0
2. `cargo test` exits 0 with no failures
3. All acceptance tests from the plan pass
4. No new `#[allow(...)]` suppressions introduced without a comment explaining why

Report completion only after running these checks. Do not report done speculatively.

### Acceptance bar

All PRs must pass `cargo build` and `cargo test` with zero failures and zero new warnings.

Every new feature or bug fix must be accompanied by tests. Choose the appropriate layer:

- **Unit test** — pure logic, error types, anything not requiring the public API boundary.
- **Integration test** (`crate/tests/`) — behaviour through the public API of a single crate.
- **End-to-end test** (`clank-shell/tests/golden/`) — binary-level: add a directory with a `stdin`
  file; run `UPDATE_GOLDENFILES=1 cargo test --test golden` to capture expected output.

Prefer a lower layer (unit > integration > e2e) for speed and isolation.
See `dev-docs/testing.md` for full testing structure, `MockHttpClient` usage, and known limitations.

## Development Workflow

See `dev-docs/workflow.md` for the full document lifecycle, frontmatter schema, and rules.

Summary: Issue → Research → Plan (proposed) → **human approves** → Implement → Acceptance tests pass → Realized design (proposed) → **human approves** → Closeout.

Agents write; humans gate moves into `approved/` and `done/`. Approved documents are never modified.

## Boundaries

### Always do

- Run `cargo build` and `cargo test` before reporting a task complete.
- Add tests for every new feature or bug fix (see Acceptance bar).
- Record deviations from an approved plan inline in the plan as they occur.
- Use `rustls-tls` for any new HTTP dependencies — never introduce an OpenSSL system dependency.

### Ask first

- Any change to public API boundaries (trait signatures, public struct fields, function signatures
  in `clank-core` or `clank-http`).
- Adding a new crate to the workspace.
- Introducing a new third-party dependency.
- Any action that would require force-pushing to a remote branch.

### Never do

- Modify files in `dev-docs/plans/approved/`, `dev-docs/plans/done/`, `dev-docs/issues/closed/`,
  or `dev-docs/designs/approved/`. They are immutable historical record.
- Commit secrets, API keys, or credentials of any kind.
- Use `git push --force` on a shared branch without explicit human instruction.
- Add `#[allow(...)]` suppressions without an inline comment explaining why.
- Delete or comment out a failing test to make the suite green — fix the test or the code.
- Proceed with implementation after discovering the approved plan is incorrect or incomplete;
  stop and ask instead.

## Escalation rules

When blocked, stop and ask the human rather than improvising a workaround. Always provide:
the exact point of failure, what was attempted, and why the current path is blocked. Then
wait for the human to decide next steps before continuing.

- **`cargo build` or `cargo test` fails after a fix attempt:** Report the full compiler/test output
  and stop. Do not delete files, comment out tests, or add `#[allow(...)]` to silence errors.
- **An approved plan turns out to be wrong or incomplete mid-implementation:** Stop immediately.
  Explain the discrepancy and what options are available. Wait for the human to decide whether
  to file a new issue, amend the plan, or take another course of action.
- **A design decision arises that the plan did not anticipate:** Stop and ask. Present the decision
  and the available options. Record the human's response in the plan before continuing.
- **Conflicting instructions:** Ask for clarification. State the conflict explicitly so the human
  can resolve it. Do not pick one instruction arbitrarily.
