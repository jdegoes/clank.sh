# AGENTS.md

## Project

clank.sh is an AI-native shell targeting `wasm32-wasip2` and native Rust. See `README.md` for full design documentation.

## Build & Test

### Prerequisites

A C compiler must be on `$PATH` for linking. On NixOS, enter a dev shell with `nix-shell -p gcc`
or equivalent before running Cargo commands.

### Building

The project is a standard Cargo workspace. All crates are built together from the repository root.

```bash
# Build all crates (native target)
cargo build

# Build and run the shell binary directly
cargo run --bin clank

# Check without producing artifacts (faster iteration)
cargo check

# Build a release binary
cargo build --release
```

The compiled `clank` binary is placed at `target/debug/clank` (or `target/release/clank` for
release builds).

**Dependency notes:**

- `reqwest` is configured with `rustls-tls` to avoid an OpenSSL system dependency. No `libssl-dev`
  or equivalent is required.
- `brush-core` and its dependencies (`nix`, `command-fds`, etc.) are native-only and will not
  compile for `wasm32-wasip2`. See Targets below.

### Targets

The project currently builds for native only. The `wasm32-wasip2` target is deferred until the
WASM process model is designed (see open issues). Do not attempt `cargo build --target wasm32-wasip2`
until that work is complete — it will fail because `brush-core` depends on the `nix` crate.

### Acceptance bar

All PRs must pass `cargo build` and `cargo test` with zero failures and zero new warnings.

Every new feature or bug fix must be accompanied by tests. Choose the appropriate layer:

- **Unit test** — for pure logic, error types, or anything testable without the public API boundary.
- **Integration test** (`crate/tests/`) — for behaviour exercised through the public API of a
  single crate, or for trait contract verification.
- **End-to-end test** (`clank-shell/tests/acceptance.rs`) — for behaviour observable at the binary
  level: exit codes, stdout/stderr content, shell semantics.

When in doubt, prefer a lower layer (unit > integration > e2e) for speed and isolation. Add an
e2e test whenever a plan's acceptance criteria can be expressed as binary behaviour.

### Testing structure

Tests are organised in three layers. Each layer has a distinct scope and lives in a specific location.

#### 1. Unit tests — inline `#[cfg(test)]` modules

Live in the same file as the code under test. Used for:

- Pure logic (error types, data transformations, Display impls)
- Constructor smoke tests
- Any test that does not require the public API boundary

#### 2. Integration tests — `crate/tests/*.rs`

Each file in a crate's `tests/` directory is compiled as a separate binary with access to the
crate's public API only. Used for:

- `clank-core/tests/` — `Repl` behaviour driven through injectable I/O (`Cursor<&[u8]>` as input,
  `Vec<u8>` as prompt output)
- `clank-http/tests/` — `HttpClient` trait contract tests using `MockHttpClient`; demonstrates the
  `Arc<dyn HttpClient>` injection pattern all callers must follow

**`MockHttpClient`** is defined in `clank-http/tests/http_client.rs`. It records every call and
returns a canned response. Copy or re-export it when writing tests that need HTTP without a real
network.

#### 3. End-to-end binary tests — `clank-shell/tests/acceptance.rs`

Drives the compiled `clank` binary via `std::process::Command` using `assert_cmd` and `predicates`.
Used for:

- Acceptance criteria from implementation plans (exit codes, stdout content)
- Regression tests for shell behaviour visible at the binary level

#### Injectable I/O contract

`Repl::run()` accepts `impl BufRead` (command input) and `impl Write` (prompt output). Command
output from brush-core builtins flows through the real process stdout/stderr and is not captured
by these parameters. To assert on command output, use the binary-level tests in layer 3.

#### Known limitations

- `exit N` with a non-zero argument does not propagate the exit code to the OS in the current
  brush-core integration. The corresponding test is commented out in `acceptance.rs` pending a
  process-model redesign.

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
