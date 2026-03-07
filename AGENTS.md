# AGENTS.md

## Project

clank.sh is an AI-native shell targeting `wasm32-wasip2` and native Rust. It embeds the Brush
bash-compatible interpreter and exposes prompts, MCP tools, and Golem agents as ordinary CLI
commands. See `README.md` for full design documentation.

**Current development target: native only.** The `wasm32-wasip2` target is the long-term
deployment goal but is deferred until the feature set is more complete. Do not attempt to compile
for WASM or add WASM-specific code until Phase 0 is explicitly scheduled.

---

## Build & Test

The project is a Cargo workspace. All active development targets native only.

```sh
# Build
cargo build

# Run all tests
cargo test

# Run a single test by name
cargo test <test_name>

# Run tests in a specific crate
cargo test -p <crate-name>

# Lint
cargo clippy --all-targets -- -D warnings

# Format check
cargo fmt --check

# Format (auto-fix)
cargo fmt

# Regenerate scenario fixture expected output after an intentional output change
CLANK_UPDATE=1 cargo test --test scenario
```

No `#[cfg(target_arch)]` or `#[cfg(target_os)]` guards in any crate. WASM target support is
deferred — do not add target-conditional code until Phase 0 is scheduled.

---

## Code Conventions

### Language and targets

- Rust only. No unsafe unless unavoidable and documented.
- Compile-time targets: native (current target) and `wasm32-wasip2` (deferred — see project statement above).
- Target-specific code is confined to abstraction-boundary crates (e.g. `clank-http`). No
  `#[cfg(target_arch)]` or `#[cfg(target_os)]` at call sites.

### Naming

| Thing | Convention | Example |
|---|---|---|
| Crates | `kebab-case` | `clank-http` |
| Traits | `PascalCase` | `HttpClient` |
| Structs / enums | `PascalCase` | `NativeHttpClient`, `HttpError` |
| Methods / fields | `snake_case` | `send_request` |
| Files / directories | `kebab-case` | `http-client.rs`, `dev-docs/` |
| CLI commands / flags | `kebab-case` | `prompt-user`, `--input-schema` |
| Manifest fields | `kebab-case` | `execution-scope`, `authorization-policy` |
| Doc slugs | `kebab-case` | `lexer-unicode-support.md` |
| YAML frontmatter keys | `snake_case` | `realized_design`, `author` |
| Dates | ISO 8601 | `2026-03-06` |

### Types and traits

- Use `Arc<dyn Trait>` for dependency injection; never inject concrete types across crate
  boundaries.
- Define async traits with `#[async_trait]` from the `async_trait` crate.
- Error types are typed enums with distinct variants — never stringly typed.
  ```rust
  pub enum HttpError {
      ConnectionFailed(/* … */),
      Timeout,
      NonSuccessResponse { status: u16, body: String },
      Tls(/* … */),
  }
  ```
- Return `Result<T, E>` throughout. Never panic in library code.
- Mark all thread-safe trait objects with `Send + Sync`.

### Imports

- Use explicit `use` paths; avoid glob imports except `use super::*` in test modules.
- Group imports: std → external crates → internal crates/modules, separated by blank lines.
- Re-export public API from each crate's `lib.rs` so callers import from the crate root.

### Formatting

- `cargo fmt` (rustfmt defaults) is enforced. No manual overrides.
- Maximum line length: 100 characters. Prefer shorter.

### Error handling

- Propagate with `?`. No `unwrap()` or `expect()` outside tests; tests use `-> anyhow::Result<()>`.
- All HTTP errors are logged to `/var/log/http.log`. Exiting `0` on failure is a bug.
- Map every error to the correct exit code (table below). Do not conflate codes.
- Surface informative messages; never hide constraints behind a generic error.

#### Exit codes

| Code | Meaning |
|---|---|
| `0` | Success |
| `1` | General error |
| `2` | Invalid usage / bad arguments |
| `3` | Timeout (model call, agent invocation, MCP tool call) |
| `4` | Remote call failed (HTTP error or connection failure) |
| `5` | Authorization failure |
| `6` | Malformed JSON when `--json` output expected; emit raw response to stderr |
| `7` | Golem not available |
| `126` | Command not executable |
| `127` | Command not found |
| `130` | Interrupted (Ctrl-C) |

### Output discipline

- Primary result → `stdout`. Warnings, traces, prompts, errors → `stderr`.
- When `--json` is requested but the output is not valid JSON, exit `6` and emit the raw response
  to `stderr`. Never swallow.

### Testing

Testing is not optional. Every new non-trivial behaviour ships with tests. A phase is not
complete until `cargo test` passes with zero failures and the tests cover the acceptance criteria
stated in the plan.

Three levels of tests are used. **Always prefer the lowest level that can cover the behaviour.**
System tests are the slowest, hardest to debug, and most brittle — use them only for things that
genuinely cannot be tested without a real process.

#### Level 1 — Unit tests

**Location:** `#[cfg(test)] mod tests { ... }` at the bottom of the source file under test.

**Scope:** A single type, function, or module in isolation. No I/O, no subprocess spawning, no
real network calls. Dependencies are injected as fakes via trait objects.

**When required:** Every non-trivial type or function. If a type has behaviour — parsing, state
transitions, error mapping, calculations — it has unit tests. Examples that must have unit tests:

- `Transcript`: append, redact, window boundary, compaction trigger, `context clear/trim`
- `HttpError`: conversion from `reqwest::Error`, display strings
- `ProcessResult`: exit code mapping
- `CommandManifest`: parsing, validation
- Dispatch table: register, deregister, lookup, concurrent access

**Async tests:** Use `#[tokio::test]` for any test that `await`s. Use
`#[tokio::test(start_paused = true)]` when testing time-dependent behaviour (e.g. timeouts,
rate limiting) to avoid slow tests — Tokio will fast-forward time automatically when no other
futures are ready.

**Test naming:** `test_<subject>_<condition>_<expected_outcome>`.

```
test_transcript_append_secret_is_redacted
test_http_error_timeout_maps_to_exit_3
test_dispatch_lookup_unregistered_returns_none
```

**Table-driven tests:** When the same logic needs to be exercised with multiple inputs, use a
parameterised loop rather than copy-pasted functions:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exit_code_mapping() {
        let cases = [
            (0u8,   0i32),
            (1u8,   1i32),
            (127u8, 127i32),
        ];
        for (input, expected) in cases {
            assert_eq!(map_exit_code(input), expected, "input={input}");
        }
    }
}
```

---

#### Level 2 — Crate integration tests

**Location:** `crates/<crate-name>/tests/<feature>.rs`

**Scope:** Cross-module wiring within a single crate. Tests that components compose correctly:
that a `ClankShell` built with a given set of builtins routes commands as expected, that the
transcript records the right entries after a sequence of operations, that `MockHttpClient`
intercepts the right requests from an `ask` call.

**May use:** `#[tokio::test]`, temp files (`tempfile` crate), `MockHttpClient`, `MockVfs`. No
subprocess spawning.

**File naming:** Named after the feature under test: `transcript.rs`, `shell_dispatch.rs`,
`ask.rs`, `manifest.rs`.

**Preferred over system tests for:** Any shell behaviour that can be driven by calling
`ClankShell::run_line()` directly. This is significantly faster and produces clearer failure
messages than spawning the binary.

**Shared test helpers:** If multiple test files in the same crate share setup code, put it in
`crates/<crate-name>/tests/common/mod.rs` and use it via `mod common;`. Do not duplicate
fixture code.

---

#### Level 3 — System tests

**Location:** `crates/clank/tests/<feature>.rs`

**Scope:** The compiled `clank` binary as a subprocess, driven via `assert_cmd`. Tests the
contract visible at the process boundary: stdin/stdout interaction, exit codes, signal handling,
multi-command session behaviour, shell startup.

**Use only when:** The behaviour genuinely requires a real process — e.g. verifying that a
signal does not cause a panic, that a specific sequence of stdin lines produces the right
combined stdout, or that the binary exits with the correct code under an error condition.

**Do not use for:** Logic that can be covered at Level 1 or 2. If a test uses `assert_cmd` but
doesn't actually need a subprocess, move it down a level.

**File naming:** Named after the user-visible feature: `shell_basics.rs`, `ask.rs`, `mcp.rs`,
`authorization.rs`.

**Assertions:** Always assert on both stdout and stderr explicitly. A test that only checks exit
code is insufficient. A test that only checks stdout misses the stderr contract.

---

#### Mocks and fakes

All mocks live in the crate that owns the trait. Consumer crates never define their own mocks
for traits they do not own.

| Mock | Crate | What it does |
|---|---|---|
| `MockHttpClient` | `clank-http` | Records requests; returns queued `MockResponse` values in FIFO order. Panics on underflow. |
| `MockVfs` | `clank-vfs` | In-memory `HashMap<PathBuf, Vec<u8>>`; returns `VfsError::NotFound` for absent paths. |

Both are public types, always available to any crate as a `dev-dependency`. No mock lives behind
`#[cfg(test)]` in a way that prevents external crates from using it.

When writing a mock for a new trait, follow the same pattern: record inputs, return pre-queued
outputs, panic on underflow with a message that names the mock and the missing response.

No test-only code outside `#[cfg(test)]` blocks or `[dev-dependencies]`.

---

#### Mandatory coverage — what must be tested at each level

| Behaviour | Required level |
|---|---|
| A pure function or data transformation | Unit (Level 1) |
| A type's state machine | Unit (Level 1) |
| Error type conversions and display | Unit (Level 1) |
| A new `Process` implementation | Unit (Level 1) + Crate integration (Level 2) |
| A new builtin command | Crate integration (Level 2) via `run_line()` |
| A new `ask` flag or option | Crate integration (Level 2) with `MockHttpClient` |
| Exit code contract for a command | Crate integration (Level 2) |
| stdout/stderr discipline | Crate integration (Level 2) or System (Level 3) |
| Stdin/stdout session involving multiple commands | System (Level 3) |
| Signal handling or process-level crash safety | System (Level 3) |
| A new top-level user-visible feature | All three levels |

---

---

#### Scenario tests

Scenario tests catch regressions in the exact output (stdout, stderr) produced by the `clank`
binary and in the filesystem state it writes (e.g. config files). Each test case is a single
YAML file that captures input state, a stdin command sequence, expected output, and expected
resulting state.

**Location:** `crates/clank/tests/scenarios/<feature>/<case>.yaml`

**Test runner:** `crates/clank/tests/scenario.rs` — runs as `cargo test --test scenario`.

**Fixture format:**

```yaml
# Human-readable description (optional but recommended).
desc: "echo produces output prefixed with the shell prompt"

# Environment variables injected into the process.
# {config} expands to an isolated per-test config file path (always set by default).
# {cwd} expands to the sandbox directory.
env:
  CLANK_CONFIG: "{config}"   # default; set automatically if not specified

# Initial config file contents as a TOML structure (optional).
# If absent, no config file exists before the test.
config:
  providers:
    anthropic:
      api_key: "sk-test"

# Files to pre-populate in the sandbox (optional).
files:
  "scripts/hello.sh": "#!/bin/sh\necho hello\n"

# Commands sent to stdin. Use YAML literal block scalar (|) for multi-line.
stdin: |
  echo "hello world"

# Expected stdout (exact match). Omit to skip assertion.
stdout: "$ hello world\n$ \n"

# Expected stderr (exact match). Omit to skip assertion.
stderr: ""

# Expected config file fields after the session (subset match — only listed
# fields are checked). Omit to skip.
config_after:
  providers:
    ollama:
      base_url: "http://localhost:11434"

# Expected files in the sandbox after the session (exact match per file).
files_after:
  "out.txt": "hello\n"
```

**Config isolation:** Every test automatically gets an isolated `CLANK_CONFIG` path pointing to
a temporary directory. Tests never read from or write to `~/.config/ask/ask.toml`.

**Workflow:**

```sh
# Run scenario tests
cargo test --test scenario

# Regenerate expected output after an intentional change
CLANK_UPDATE=1 cargo test --test scenario

# Filter to fixtures whose path contains a substring
cargo test --test scenario -- scenario_tests echo
```

**When to add a scenario fixture:**

Add a scenario fixture for every new user-visible command or flag once its output is stable.
Scenario fixtures are the regression net for the binary's external contract — if output or
config-write behaviour changes unintentionally, the fixture catches it. If it changes
intentionally, run `CLANK_UPDATE=1`, review the diff, and commit the updated fixtures.

**What scenario fixtures cover vs. what they do not:**

| Covered | Not covered |
|---|---|
| Exact stdout/stderr for stable commands | Internal logic (use unit/integration tests) |
| Config file written after a command | Timing or performance |
| Multi-command session behaviour | Signal handling (use system tests) |
| Config isolation (always enforced) | Live network calls |

---

#### Before marking a task complete

A task in a plan's `## Tasks` checklist is not done until:

1. `cargo test` passes with zero failures.
2. `cargo clippy --all-targets -- -D warnings` passes.
3. `cargo fmt --check` passes.
4. New logic has tests at the appropriate level from the table above.
5. Tests exercise the failure paths, not just the happy path.

Stubbed or `todo!()` implementations with no tests do not count as complete.

### Golem / WASM portability

- HTTP: `reqwest` on native, `wstd` on `wasm32-wasip2` — both hidden behind `HttpClient` in
  `clank-http`. No direct HTTP calls elsewhere.
- `nix` crate only in native-only paths guarded at the abstraction boundary.
- Golem-dependent features fail with a precise, informative error when Golem is unavailable.

---

## Development Workflow

All development artifacts live in `dev-docs/`. Everything is a Markdown file with YAML frontmatter.

### Document types

| Type | Location | Purpose |
|---|---|---|
| Research | `dev-docs/research/` | Raw investigation and prior art. Research informs but does not decide. |
| Design | `dev-docs/designs/proposed/` or `approved/` | Specifications for system areas. Approved designs are frozen permanent record. |
| Issue | `dev-docs/issues/open/` or `closed/` | What needs to be built or fixed, and why. No solution detail. |
| Plan | `dev-docs/plans/proposed/`, `approved/`, or `done/` | How an issue will be resolved. Full provenance: originating issue, research consulted, designs referenced, developer feedback on design decisions, acceptance tests. |

### Frontmatter schema

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

Fields left blank at document creation are filled in as the lifecycle progresses. Agents fill them
at the step where the referenced artifact is created.

Files are named in `kebab-case`. Plans and issues should use a short descriptive slug,
e.g. `lexer-unicode-support.md`.

### Lifecycle

1. **Issue created** in `dev-docs/issues/open/`. States the problem or capability gap. Never
   modified to include solution detail.
2. **Research conducted** as needed. Written to `dev-docs/research/`. If design docs for the
   affected area are missing, write a proposed design and proceed with the plan, noting in the plan
   that the design is pending human approval.
3. **Plan written** in `dev-docs/plans/proposed/`. Consult the developer on significant design
   decisions and record their feedback in the plan. The plan references the originating issue, all
   research and designs consulted, feedback received, and acceptance tests. Include a `## Tasks`
   section with a checkbox list of concrete implementation steps.
4. **Human approves plan** by moving it to `dev-docs/plans/approved/`. No implementation before
   this. Any proposed design from Step 2 is also approved or rejected at this point.
5. **Implementation proceeds.** Check off tasks as they complete. Note deviations inline. If the
   approved plan turns out to be incorrect or incomplete, stop and file a new issue.
6. **Acceptance tests pass.** Once all pass, write a complete realized design doc to
   `dev-docs/designs/proposed/`. Do not summarize it for the human; it will be reviewed in full.
7. **Human approves realized design** → `dev-docs/designs/approved/`. The realized design
   supersedes the approved design for future reference; the original remains as historical record.
8. **Closeout:** plan → `dev-docs/plans/done/`. Issue → `dev-docs/issues/closed/`. Both immutable.

### Rules

- Agents write; humans gate moves into `approved/` and `done/`.
- Approved documents are never modified. They are permanent historical record.
- The code is the ground truth for current system state. Design docs record intent and decisions at
  a point in time, not a live mirror of the code.
- Agents must never modify files in `dev-docs/plans/approved/`, `dev-docs/plans/done/`,
  `dev-docs/issues/closed/`, or `dev-docs/designs/approved/`.
- Agents must not create a plan without an originating issue in `dev-docs/issues/open/`.

### !! Workflow file movements — mandatory procedure !!

**Agents must never move `dev-docs/` files directly.** All lifecycle transitions (approvals,
closeouts, archiving) require an explicit `mv` command executed by a human.

When a lifecycle transition is due, the agent must:

1. **Write a summary** of every file movement required, stating clearly what is being moved,
   from where, and to where, and why.
2. **Write a shell script** at the repository root (e.g. `workflow-closeout-<name>.sh`) that
   performs all the `mv` commands and deletes itself on completion.
3. **State the script path** and ask the human to review and approve it before execution.
4. **Wait for explicit approval.** Do not run the script, do not move any files, and do not
   proceed until the human confirms.

Example output format:

```
The following workflow movements are required to close out <phase>:

  dev-docs/plans/approved/foo.md       → dev-docs/plans/done/foo.md
  dev-docs/issues/open/foo.md          → dev-docs/issues/closed/foo.md
  dev-docs/designs/proposed/foo-realized.md → dev-docs/designs/approved/foo-realized.md

I have written these as a script to `workflow-closeout-foo.sh`.
Please review it and let me know when I should run it.
```

This rule applies to all workflow transitions without exception, including plan approval,
realized design approval, issue closeout, and plan closeout.
