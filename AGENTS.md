# AGENTS.md

## Project

clank.sh is an AI-native shell targeting `wasm32-wasip2` and native Rust. See `README.md` for full design documentation.

## Build & Test

Run from the workspace root:

- `cargo build`
- `cargo test`
- `cargo clippy --all-targets -- -D warnings`

## Testing

Every feature and bug fix must have tests at all applicable levels.

| Layer | Location | Use for |
|---|---|---|
| Unit | `clank/src/**/#[cfg(test)]` | Internal logic, private functions. Use `#[tokio::test]` for async. |
| Integration | `clank/tests/<concern>.rs` | Public shell behaviour via process spawn (`assert_cmd`). Shared helpers in `clank/tests/common/mod.rs`. |
| System | `clank/tests/system.rs` | Multi-step scenarios with shared shell state. Name tests `scenario_*`. |

New builtins or shell features → add to the relevant `clank/tests/<concern>.rs` or create one.
New multi-step compositions → add a `scenario_*` test to `clank/tests/system.rs`.

### Behavioural equivalence for internal command implementations

Every command implemented internally in `clank-builtins` must produce output
behaviorally equivalent to the corresponding OS command for the same inputs.
Golden tests enforce this: expected output is derived from what the real OS command
produces, and the internal implementation must match it. Any divergence in listing,
formatting, ordering, or exit code is a test failure.

### WASM compatibility

All code in `clank-builtins` and all dependencies it introduces must compile to
`wasm32-wasip2`. Do not introduce crates that depend on `nix`, `libc`, OS process
spawning, or Unix-only system calls. Use `std::fs`, `std::io`, and WASM-compatible
crates only. Gate any Unix-specific code behind `#[cfg(unix)]`.

## Code Conventions

_To be filled in._

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

