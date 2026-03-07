# AGENTS.md

## Project

clank.sh is an AI-native shell targeting `wasm32-wasip2` and native Rust. See `README.md` for full design documentation.

> **WASM target deferred.** The `wasm32-wasip2` build target is blocked by `brush-core`'s hard dependencies on Unix-only system crates (`nix`, Tokio process/signal APIs, `procfs`). All implementation work targets the native build only until this is resolved. See `dev-docs/issues/open/brush-wasm-portability.md` for the full context and resolution options. Do not attempt to introduce `wasm32-wasip2` builds, `cargo-component`, Golem WIT, or `wstd` until that issue is closed.

## Build & Test

### Prerequisites

- Rust stable toolchain (1.87.0 or later). The `rust-toolchain.toml` at the workspace root pins the channel; `rustup` will install the correct version automatically on first use.
- No other system dependencies are required for the native build.

### Build

```sh
# Build all workspace crates (native target):
cargo build --workspace

# Build only the clank binary:
cargo build -p clank-shell
```

The compiled binary is at `target/debug/clank`.

### Test

The workspace has three test tiers. All three run with a single command:

```sh
cargo test --workspace
```

**Tier 1 — Unit tests** (`clank-core/src/lib.rs`, `clank-http/src/lib.rs`)
Inline `#[cfg(test)]` modules. Run as part of `cargo test --workspace` or individually:
```sh
cargo test -p clank-core
cargo test -p clank-http
```

**Tier 2 — Integration tests** (`clank-core/tests/`)
Compiled against the `clank-core` library. Exercise the public shell API — `Shell::new`, `default_options`, `run`, `run_with_options` — without spawning a subprocess:
```sh
cargo test -p clank-core
```

**Tier 3 — Acceptance tests** (`clank-acceptance/`)
Spawn the compiled `clank` binary as a subprocess and assert on stdout, stderr, and exit code. Test cases are YAML files under `clank-acceptance/cases/`. The binary must be built before running acceptance tests; `cargo test --workspace` handles this automatically.
```sh
# Acceptance tests only:
cargo build -p clank-shell && cargo test -p clank-acceptance
```

To add a new acceptance test case, drop a `.yaml` file anywhere under `clank-acceptance/cases/`. No code changes required.

### Lint and Format

```sh
cargo clippy --workspace --tests -- -D warnings
cargo fmt --all --check
```

To auto-fix formatting:
```sh
cargo fmt --all
```

## Conventions

### Code Style

- Follow existing code conventions in the file you are editing.
- Do not add unnecessary comments. Code should be self-explanatory; comments should explain *why*, not *what*.
- Use existing libraries and utilities from the codebase before reaching for something new.
- Never expose or log secrets or keys.

### Dependencies

- Always ask before adding a new third-party crate. Present the crate name, version, purpose, and why nothing already in the workspace satisfies the need. Wait for explicit approval before adding it.

### Tests

- Never comment out, delete, or mark a test as `#[ignore]` without explicit approval.

### Git

- Never run `git push --force`.

### Documentation and Historical Records

- Never modify files under `dev-docs/plans/approved/`, `dev-docs/plans/done/`, `dev-docs/issues/closed/`, or `dev-docs/designs/approved/`. They are immutable historical records.

### Technical Objectivity

- Do not blindly accept that what the human says is correct. Humans make mistakes. If a proposed approach appears wrong, inefficient, or likely to lead to poor design outcomes, say so directly. Back the objection with concrete reasoning and evidence — compiler behaviour, crate API constraints, benchmarks, precedent in the codebase, or relevant prior art. Agreeing with a bad idea to be agreeable is more harmful than a respectful disagreement.
- Prioritise technical accuracy over validation. The goal is the best outcome for the project, not the most comfortable conversation.

### Handling Blockers and Plan Deviations

- If you get blocked, stop immediately and ask. Do not invent or implement a workaround without explicit approval. Present: the exact point of failure, what was attempted, and why you are blocked. Wait for direction.
- If an approved plan turns out to be wrong or incomplete — unexpected behaviour, missing information, or contradictory constraints discovered during implementation — stop immediately. Explain the issue and present the available options (new issue, plan amendment, design revision, or other). The human decides how to proceed.

## Development Workflow

All development artifacts live in `dev-docs/`. Everything is a Markdown file with YAML frontmatter.

### Document Types

| Type | Location | Purpose |
|---|---|---|
| Research | `dev-docs/research/` | Raw investigation and prior art. Research informs but does not decide. |
| Design | `dev-docs/designs/proposed/` or `approved/` | Specifications for system areas. Approved designs are frozen permanent record. |
| Issue | `dev-docs/issues/open/` or `closed/` | What needs to be built or fixed, and why. No solution detail. |
| Plan | `dev-docs/plans/proposed/`, `approved/`, or `done/` | How an issue will be resolved. Full provenance: originating issue, research consulted, designs referenced, developer feedback on design decisions, acceptance tests. |

### Frontmatter Schema

Every document begins with YAML frontmatter. Required fields vary by type:

| Field | Research | Design | Issue | Plan |
|---|---|---|---|---|
| `title` | required | required | required | required |
| `date` | required | required | required | required |
| `author` | required | required | required | required |
| `issue` | — | — | — | required — path to originating issue |
| `research` | — | — | — | required if applicable — list of paths to research docs consulted |
| `designs` | — | — | — | required if applicable — list of paths to design docs referenced |
| `closed` | — | — | optional — date closed | — |
| `plan` | — | — | optional — path to plan | — |
| `completed` | — | — | — | optional — date all tasks completed |
| `realized_design` | — | — | — | optional — path to realized design doc |

Lifecycle state is encoded in directory position, not frontmatter. There is no `status` field.

For agent-authored documents, use `author: agent`.

Use ISO 8601 for all dates: `YYYY-MM-DD`.

Fields left blank at document creation are filled in as the lifecycle progresses. Agents fill them at the step where the referenced artifact is created.

Files are named in `kebab-case`. Plans and issues should use a short descriptive slug, e.g. `lexer-unicode-support.md`.

### Lifecycle

1. **Issue created** in `dev-docs/issues/open/`. States the problem or capability gap. Never modified to include solution detail.
2. **Research conducted** as needed. Written to `dev-docs/research/`. If design docs for the affected area are missing, write a proposed design and proceed with the plan, noting in the plan that the design is pending human approval.
3. **Plan written** in `dev-docs/plans/proposed/`. Before writing, consult the developer on any significant design decisions and record their feedback in the plan. The plan references: originating issue, all research consulted, all relevant designs, feedback received on design decisions, and acceptance tests. The plan must include a `## Tasks` section with a checkbox list of concrete implementation steps.
4. **Human approves plan** by moving it to `dev-docs/plans/approved/`. No implementation begins before this. If a proposed design was written in Step 2, the human approves or rejects it at this point before approving the plan. If the proposed design is rejected, the agent revises it based on human feedback and resubmits before the plan proceeds.
5. **Implementation proceeds.** Checkboxes checked as tasks complete. Deviations from the plan noted inline as they occur. If implementation reveals the approved plan is incorrect or incomplete, stop and file a new issue rather than proceeding unilaterally.
6. **Acceptance tests pass.** If only partially passing, continue implementation until all pass before proceeding. Once all pass, agent writes a complete realized design doc to `dev-docs/designs/proposed/`. Do not summarize the realized design for the human; it will be reviewed in full.
7. **Human approves realized design** by moving it to `dev-docs/designs/approved/`. The realized design supersedes the approved design for future reference. When writing future plans, cite the realized design. If no realized design exists for an area, cite the most recent approved design. The original approved design remains as permanent record of intent.
8. **Closeout:** plan moved to `dev-docs/plans/done/`. Issue moved to `dev-docs/issues/closed/`. Both are immutable from this point.

### Rules

- Agents write; humans gate moves into `approved/` and `done/`.
- Approved documents are never modified. They are permanent historical record.
- The code is the ground truth for current system state. Design docs record intent and decisions at a point in time, not a live mirror of the code.
- Agents must never modify files in `dev-docs/plans/approved/`, `dev-docs/plans/done/`, `dev-docs/issues/closed/`, or `dev-docs/designs/approved/`.
- Agents must not create a plan without an originating issue in `dev-docs/issues/open/`.

