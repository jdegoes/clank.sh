# Session Handover

**Date:** 2026-03-07  
**Project:** clank.sh — AI-native bash-compatible shell  
**Repo root:** `/Users/james/Development/Ziverge/2026-03-06-agentic-training-2/clank.sh`

---

## Current project state

**309 tests pass** (`cargo test --workspace` — zero failures). Clippy and fmt clean.

**Completion estimate:** ~42% of total feature surface.

---

## What was done in this session (pre-review quality pass)

This session was a systematic pre-review quality and correctness pass, not a feature phase.
The following work was completed:

| Item | Description |
|---|---|
| **Code quality remediation (pre-review)** | 15 issues fixed: `context summarize` now uses `AnthropicProvider` instead of hand-rolled HTTP; `ModelOutput`/`AskOutput` named return structs; `new()` constructors on all command structs; private fields; `write!`/`writeln!` replacing `push_str(&format!(...))` ; `argv.clone()` borrow workaround eliminated; `buf.clone()` in REPL eliminated; `spawn_blocking` for Confirm prompt; `tracing::warn!` on capture file failure; `#[allow(dead_code)]` removed; exit codes corrected |
| **Provider wire format fix** | OpenRouter and OpenAI-compat now send system prompt as `messages[0]` with `role: "system"` per OpenAI spec — previously silently dropped by both providers, breaking transcript context |
| **Authorization model corrected** | `ExecutionContext { User, Agent }` introduced; `Confirm`/`SudoOnly` policies now only enforced in agent context; user-typed commands run freely; `run_line_as_agent()` added as Phase 3 entry point |
| **Working directory fix** | `ProcessContext.cwd` added; all 7 VFS commands resolve relative paths against Brush's internal working dir, not OS process cwd; `cd` then `mkdir`/`cat`/`ls` etc. now work correctly |
| **`VfsError` display fixed** | `NotFound` now displays `No such file or directory`; `PermissionDenied` displays `permission denied` — POSIX convention, no redundant path repetition |
| **`ask` system prompt corrected** | Removed agentic language ("available tools", "executing commands") from Phase 1 system prompt; model now acts as a session-context reader not an agent |
| **Tutorial conformance scenario suite** | 18 scenario fixtures in `tests/scenarios/tutorial/` covering every automatable tutorial snippet from §2–§11 |
| **Tutorial corrections** | §8 `/proc/` example corrected (PID entries only live while process is running); §9 authorization rewritten to reflect user-vs-agent model; `context summarize` provider restriction documented |
| **Retrospective updated** | D12 (auth enforcement fix), D13 (`sudo ask` propagation gap), `VfsError` display documented; `sudo ask` methodology note added |

---

## Workflow state

### `dev-docs/issues/open/`

| File | Description | Priority |
|---|---|---|
| `sudo-ask-broad-authorization.md` | `sudo ask` doesn't propagate broad auth to agent commands | **Before Phase 3** |
| `tutorial-proc-cmdline-snippet-incorrect.md` | Closed by tutorial correction this session; issue file remains for record | Fixed |
| `authorization-context-user-vs-agent.md` | Architectural issue — two-context auth model; D12 resolved the Phase 1 case | Phase 3 design |
| `authorization-bypass-for-user-context-phase1.md` | Resolved by D12 fix this session | Fixed |
| `cd-fails-after-mkdir.md` | Resolved by working directory fix this session | Fixed |
| `transcript-not-passed-to-model.md` | Resolved by wire format fix this session | Fixed |
| `ask-system-prompt-causes-hallucinated-agentic-behaviour.md` | Resolved by system prompt fix | Fixed |
| `phase-0-foundation.md` | WASM target — explicitly deferred | Deferred |
| `phase-3-packages-and-mcp.md` | `grease`, MCP, tab completion, remaining Unix commands | Phase 3 |
| `phase-4-golem-integration.md` | Golem adapter, durable state, `ask repl` | Phase 4 |
| `phase-5-polish.md` | Signed registry, auto-compaction, TUI, `man` pages | Phase 5 |

### `dev-docs/plans/approved/`

Plans from this session that are implemented but not yet formally closed out:

| File | Description |
|---|---|
| `pre-review-code-quality.md` | All 12 tasks complete |
| `transcript-not-passed-to-model.md` | All tasks complete |
| `ask-system-prompt-fix.md` | All tasks complete |
| `authorization-user-vs-agent-phase1.md` | All tasks complete |
| `cd-mkdir-working-directory-fix.md` | All tasks complete |
| `tutorial-conformance-scenarios.md` | All tasks complete |

> **Note for next agent:** These plans should be formally closed out (realized design → plan → done, issues → closed) via workflow shell scripts before beginning Phase 3 work.

### `dev-docs/plans/proposed/` — empty

---

## What is built and working

### Shell primitives
- Bash-compatible scripting (Brush embedded)
- Session transcript: captures all commands and output including OS-fallthrough
- `context show / clear / summarize / trim`
- Synthetic process table, `ps` / `ps aux` / `ps -ef`
- Virtual `/proc/<pid>/cmdline`, `/proc/<pid>/status`, `/proc/<pid>/environ` (live while process runs)
- `/proc/clank/system-prompt` (static; becomes dynamic in Phase 3)
- Authorization: `Allow` / `Confirm` / `SudoOnly` — enforced for agent context only
- `ExecutionContext { User, Agent }` — `run_line()` for user, `run_line_as_agent()` for Phase 3
- `export --secret` with `SecretsRegistry`
- `env` (current environment, secrets masked as `***`)
- `prompt-user` with `--choices`, `--confirm`, `--secret`, Markdown rendering; exit 130 on Ctrl-C/EOF
- `cd` + relative-path VFS commands work correctly (cwd threaded through `ProcessContext`)

### AI integration (`clank-ask`)
- `ask` with `--fresh`, `--no-transcript`, `--inherit`, `--model`, `--json`
- Piped stdin as supplementary context
- AI response appended to transcript
- System prompt: session-context assistant (Phase 1); no agentic tool-calling language
- Four providers: Anthropic, OpenRouter, Ollama, OpenAI-compatible
- OpenRouter and OpenAI-compat correctly send system prompt as `messages[0] role: "system"`
- `model add <provider> [--key <key>] [--url <url>]` for all four providers
- `model default [<model>]` — get/set default model
- `model list` — shows providers with `base_url` and `api_key` status
- `model remove` / `model info` — stubs (exit 2, planned)
- `CLANK_CONFIG` env var override

### Implemented commands (VFS-backed, relative paths resolve against Brush cwd)
`ls` (`-a`, `-l`, combined), `cat`, `grep` (`-i`, `-n`, `-l`, `-r`, combined), `stat`,
`mkdir` (`-p`), `rm` (`-r`, `-f`, combined), `touch`, `env`, `ps`, `export`

### Infrastructure
- `Vfs` trait with full read + write; `MockVfs`; `RealFs`; `LayeredVfs` with `/proc/` mount
- `ProcessContext.cwd: PathBuf` — Brush working dir at dispatch time
- `commands::resolve(cwd, path)` — path resolution helper used by all VFS commands
- `MockHttpClient`; `NativeHttpClient`
- Scenario test harness: `crates/clank/tests/scenario.rs` + `tests/scenarios/**/*.yaml`
- `ModelOutput` / `AskOutput` typed return structs on public API
- `new()` constructors with private fields on all command process structs
- `ExecutionContext` thread-local for user-vs-agent dispatch

---

## Crate structure

```
crates/
  clank/             Binary entry point; AskProcess + ModelProcess adapters
  clank-ask/         ask/model commands; provider impls; ModelOutput/AskOutput; AskConfig
  clank-shell/       Shell core: Brush integration, transcript, builtins, dispatch, commands
  clank-http/        HttpClient trait; NativeHttpClient; MockHttpClient
  clank-vfs/         Vfs trait (read + write); MockVfs; RealFs; LayeredVfs; ProcHandler
  clank-manifest/    Command manifest types; authorization policies; global registry
  clank-golem/       Stub
  clank-grease/      Stub
```

---

## Key architectural decisions this session

**`ExecutionContext { User, Agent }`.** Carried via thread-local alongside `ACTIVE_SHELL_ID`.
`run_line()` sets `User`; `run_line_as_agent()` (Phase 3 entry point) sets `Agent`. The two
enforcement points in `shell.rs` (SudoOnly early deny) and `builtins.rs` (Confirm prompt)
are gated on `Agent` context, with `TODO(Phase 3)` comments at each site.

**Agent sudo semantics per spec.** Agents cannot use `sudo` — a `sudo` prefix in agent
context is an immediate deny (exit 5). `SudoOnly` commands are always denied in agent context.
Broad elevation for agents comes only from `sudo ask` at the human level — not yet implemented
(tracked as D13, must be done in Phase 3).

**`ProcessContext.cwd`.** Populated from `ctx.shell.working_dir()` in `dispatch_builtin`
before the `async move` block consumes `ctx`. All seven VFS commands call `resolve(cwd, path)`
before passing paths to the Vfs. This is the only safe source of truth — `std::env::current_dir()`
is the OS process cwd which is never updated when `cd` runs.

**OpenAI wire format.** The `system` top-level field is an Anthropic-specific extension.
OpenRouter and OpenAI-compat silently ignored it, dropping the entire transcript from every
`ask` request. Both providers now prepend a `role: "system"` message to `messages[]`.
Wire format spec URL is cited in both the struct doc-comment and each provider implementation.

---

## Essential reading — in order

1. **`AGENTS.md`** — coding conventions, testing levels, workflow rules (mandatory)
2. **`dev-docs/HANDOVER.md`** — this file
3. **`README.md`** — the full specification; authoritative for Phase 3 planning
4. **`OVERVIEW.md`** — current state in one page
5. **`TUTORIAL.md`** — user-facing guide; now includes tutorial conformance scenarios
6. **`dev-docs/retrospective.md`** — all spec deviations; read before Phase 3 planning
7. **`dev-docs/issues/open/phase-3-packages-and-mcp.md`** — next phase scope
8. **`dev-docs/designs/approved/`** — realized designs for all completed phases

---

## Deviations from spec still open

| Deviation | Status | Resolution |
|---|---|---|
| D1: `/proc/clank/system-prompt` static | Open | Phase 3 (package registry) |
| D2: Job control not wired to clank ps | Open | Phase 3/4 |
| D3: `kill` doesn't cancel Golem invocations | Open | Phase 4 |
| D5: No automatic transcript compaction | Open | Phase 5 |
| D12: Auth enforcement applied to user-typed commands | **Resolved** | `ExecutionContext` fix |
| D13: `sudo ask` broad auth not propagated to agent | Open | **Must address in Phase 3** |

---

## Mandatory before Phase 3

1. **Close out this session's plans** — six plans in `approved/` need realized designs and closeout scripts
2. **D13 (`sudo ask` propagation)** — must be designed and implemented alongside agent command dispatch
3. **`prompt-user` Ctrl-C exit code (D4)** — Phase 3 makes `prompt-user` the primary confirmation mechanism

---

## Build commands

```sh
cargo build                                        # build workspace
cargo test --workspace                             # run all tests (309)
cargo test --test scenario                         # run scenario fixtures (33)
cargo clippy --all-targets -- -D warnings         # lint
cargo fmt --check                                  # format check
cargo fmt                                          # format (auto-fix)
CLANK_UPDATE=1 cargo test --test scenario         # regenerate scenario fixtures
cargo test --test scenario -- scenario_tests <filter>  # run specific scenarios
./target/debug/clank                               # run the shell
CLANK_CONFIG=./ask.toml ./target/debug/clank      # run with local config
```
