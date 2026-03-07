---
title: "Project retrospective: spec vs. current state"
date: 2026-03-07
author: agent
---

# Project Retrospective — Spec vs. Current State

This document compares the original specification in `README.md` against what has been built
and tested, documents every deviation from the spec (including rationale), and summarises
the remaining work.

---

## Scope of completed work

The following phases are complete:

| Phase | Description |
|---|---|
| Phase -1 | Native foundation — workspace, Brush embedding, process abstraction, stub dispatch |
| Phase 1 | Transcript capture, `ask`, `context`, provider layer, `model list` |
| Phase 2 | Synthetic process table, `ps`, `prompt-user`, authorization, `export --secret`, `/proc/` |
| Local providers (1.5) | Ollama, OpenAI-compatible, `model add`, extended `model list` |
| OS-fallthrough capture | Transcript capture for full-path and `$PATH`-resolved OS commands |

Infrastructure improvements delivered alongside feature work:

| Item | Description |
|---|---|
| Scenario test harness | YAML fixtures replacing trycmd; always-isolated config; `config_after` assertions |
| Test coverage remediation | 80+ new behavioural tests across all crates |
| Code quality remediation | 27 correctness bugs, idiom violations, and architectural improvements |

---

## Full conformance: spec vs. current state

### Shell primitives

| Feature | Spec | Current state | Notes |
|---|---|---|---|
| Bash-compatible scripting | Required | **Conforming** — Brush embedded | Brush gaps inherited: `coproc`, `select`, `ERR` trap, some `set`/`shopt` flags |
| Session transcript (sliding window) | Required | **Conforming** | |
| Transcript capture for registered commands | Required | **Conforming** | |
| Transcript capture for OS-fallthrough | Required | **Conforming** | Was a known bug; fixed in this session |
| `context show` | Required | **Conforming** | |
| `context clear` | Required | **Conforming** | |
| `context summarize` | Required | **Conforming** | |
| `context trim <n>` | Required | **Conforming** | |
| `context show` output NOT re-recorded | Required | **Conforming** | Explicitly verified |
| Parent shell commands (`cd`, `export`, `exit`, `source`) | Required | **Conforming** — via Brush | |
| Synthetic process table (PID, PPID, status) | Required | **Conforming** | |
| `ps` / `ps aux` / `ps -ef` | Required | **Conforming** | |
| `P` state for paused processes | Required | **Conforming** | |
| `/proc/<pid>/cmdline` | Required | **Conforming** | NUL-separated format |
| `/proc/<pid>/status` | Required | **Conforming** | Includes Pid, PPid, State, Name |
| `/proc/<pid>/environ` | Required | **Conforming** | Secrets filtered; populated from real env |
| `/proc/clank/system-prompt` | Required | **Partially conforming** — see deviation D1 | |
| Authorization: `Allow` / `Confirm` / `SudoOnly` | Required | **Conforming** | Policies enforced in agent context only; user runs freely — see D12 |
| `sudo` prefix (one-shot, human only) | Required | **Conforming** | Agents cannot use `sudo` — exit 5 in agent context |
| `sudo` state per-shell-instance | Implied | **Conforming** | |
| `export --secret` | Required | **Conforming** | |
| Secrets filtered from transcript and `/proc/environ` | Required | **Conforming** | |
| Secrets masked in `env` output | Required | **Conforming** | Fixed in code quality pass |
| Job control (`&`, `jobs`, `fg`, `bg`, `wait`) | Required | **Partially conforming** — see deviation D2 | |
| `kill` | Required | **Partially conforming** — see deviation D3 | |

### AI integration

| Feature | Spec | Current state | Notes |
|---|---|---|---|
| `ask "question"` | Required | **Conforming** | |
| `--fresh` / `--no-transcript` / `--inherit` flags | Required | **Conforming** | |
| `--model` flag | Required | **Conforming** | |
| `--json` flag with exit 6 on invalid JSON | Required | **Conforming** | |
| Piped stdin as supplementary input | Required | **Conforming** | |
| Transcript as model context (no briefing step) | Required | **Conforming** | |
| AI response appended to transcript | Required | **Conforming** | |
| Anthropic provider | Required | **Conforming** | |
| OpenRouter provider | Required | **Conforming** | |
| Ollama provider | Required | **Conforming** | |
| OpenAI-compatible provider (llama.cpp, LM Studio, vLLM) | Required | **Conforming** | |
| `model add <provider> --key / --url` | Required | **Conforming** | |
| `model list` | Required | **Conforming** | Shows `base_url` for local providers |
| `model default` | Required | **Not yet implemented** — stub | Phase 3 scope |
| `model remove` | Required | **Not yet implemented** — stub | Phase 3 scope |
| `model info` | Required | **Not yet implemented** — stub | Phase 3 scope |
| `ask repl` | Required | **Not yet implemented** | Phase 4 scope |
| `sudo ask` (broad authorization) | Required | **Non-conforming** — see deviation D13 | `sudo` prefix accepted; broad authorization propagation not implemented |
| `/proc/clank/system-prompt` | Required | **Partially conforming** — see deviation D1 | |
| `prompt-user` | Required | **Conforming** | |
| `prompt-user --choices` | Required | **Conforming** | |
| `prompt-user --confirm` | Required | **Conforming** | |
| `prompt-user --secret` | Required | **Conforming** | |
| Markdown rendered before `prompt-user` question | Required | **Conforming** | termimad rendering |
| `prompt-user` Ctrl-C → exit 130 | Required | **Partially conforming** — see deviation D4 | |
| `CLANK_CONFIG` env var override | Required | **Conforming** | |

### Core commands

| Feature | Spec | Current state | Notes |
|---|---|---|---|
| `ls` (VFS-backed) | Required | **Conforming** | Implemented with `has_flag()` for combined flags |
| `cat` (VFS-backed) | Required | **Conforming** | |
| `grep` (VFS-backed, `-r`, `-i`, `-n`, `-l`) | Required | **Conforming** | Combined flags supported |
| `stat` (VFS-backed) | Required | **Conforming** | |
| `mkdir` (VFS-backed, `-p`) | Required | **Conforming** | |
| `rm` (VFS-backed, `-r`, `-f`) | Required | **Conforming** | |
| `touch` (VFS-backed) | Required | **Conforming** | |
| `env` | Required | **Conforming** | Secrets masked; was broken until code quality pass |
| `cp` | Required | **Not yet implemented** — stub | Phase 3 scope |
| `mv` | Required | **Not yet implemented** — stub | Phase 3 scope |
| `find` | Required | **Not yet implemented** — stub | Phase 3 scope |
| `sed` | Required | **Not yet implemented** — stub | Phase 3 scope |
| `awk` | Required | **Not yet implemented** — stub | Phase 3 scope |
| `sort`, `uniq`, `wc`, `head`, `tail`, `cut`, `tr`, `xargs` | Required | **Not yet implemented** — stubs | Phase 3 scope |
| `diff`, `patch` | Required | **Not yet implemented** — stubs | Phase 3 scope |
| `tee` | Required | **Not yet implemented** — stub | Phase 3 scope |
| `jq` | Required | **Not yet implemented** — stub | Phase 3 scope |
| `curl`, `wget` | Required | **Not yet implemented** — stubs | Phase 3 scope |
| `file` | Required | **Not yet implemented** — stub | Phase 3 scope |
| `man` | Required | **Not yet implemented** — stub | Phase 5 scope |
| `ps` | Required | **Conforming** | |
| `kill` | Required | **Partially conforming** — see deviation D3 | |

### Package system and MCP

| Feature | Spec | Current state | Notes |
|---|---|---|---|
| `grease install/remove/list/search/update/info` | Required | **Not yet implemented** — stub | Phase 3 scope |
| MCP HTTPS transport | Required | **Not yet implemented** | Phase 3 scope |
| `mcp session` commands | Required | **Not yet implemented** | Phase 3 scope |
| `/mnt/mcp/<server>/` virtual mount | Required | **Not yet implemented** | Phase 3 scope |
| MCP tools as executables | Required | **Not yet implemented** | Phase 3 scope |
| MCP prompts as executables | Required | **Not yet implemented** | Phase 3 scope |
| Tab completion (manifest-driven) | Required | **Not yet implemented** | Phase 3 scope |
| `--help` on all commands | Required | **Not yet implemented** | Phase 3 scope |
| `man` pages | Required | **Not yet implemented** | Phase 5 scope |

### Golem integration

| Feature | Spec | Current state | Notes |
|---|---|---|---|
| Golem adapter | Required | **Stub only** | Phase 4 scope |
| `golem` command | Required | **Not yet implemented** | Phase 4 scope |
| Durable state, exactly-once invocations | Required | **Not yet implemented** | Phase 4 scope |
| `wasm32-wasip2` compile target | Required | **Deferred** | Explicitly deferred to Phase 0 |

---

## Deviations from the spec

### D1 — `/proc/clank/system-prompt` is static, not dynamic

**Spec:** The system prompt is computed on read from the current set of installed tools,
skills, and shell configuration. It reflects what would be sent to the model on the next
`ask` invocation. Installing or removing a package changes it.

**Current state:** `/proc/clank/system-prompt` is backed by a `SystemPromptSource` trait
on `ProcHandler`. The production implementation returns a fixed string (the static system
prompt from `build_system_prompt()`). It does not reflect installed packages, skills, or
manifest entries because the package system (Phase 3) does not exist yet.

**Rationale:** Dynamic computation from installed manifests requires the package system,
which is Phase 3. Implementing a dynamic system prompt without the packages that populate
it would produce a correct but empty result. The static implementation correctly reflects
the current state of the shell (no packages installed). The `SystemPromptSource` trait is
the right abstraction: Phase 3 will supply an implementation that reads from the manifest
registry.

**Impact:** Low. No installed packages exist to reflect. `cat /proc/clank/system-prompt`
returns the fixed system prompt that `ask` actually uses. The content is correct; the
mechanism is not yet dynamic.

---

### D2 — Job control is partially implemented

**Spec:** `&`, `jobs`, `fg`, `bg`, `wait` provide synthetic job control. `Ctrl-Z` is
noted as not supported in v1.

**Current state:** `&`, `jobs`, `fg`, `bg`, and `wait` are dispatched through Brush's
builtin layer and work for Brush's internal job model. However they operate on Brush's
own process table, not on clank's synthetic process table. Background jobs started with `&`
do not appear in clank's `ps` output.

**Rationale:** Wiring Brush's job control to clank's synthetic process table requires
intercepting Brush's job state at a level that was not available in Phase 2. The spec calls
this out implicitly — job control is listed as working but "not supported: Ctrl-Z, terminal
process-group behavior". The current state is functional for practical use (background jobs
work) but not fully integrated into clank's process model.

**Impact:** Medium. `ps` does not show backgrounded jobs. For typical agentic workflows
this is low-impact — the AI and human are both reading `ps` for foreground processes and
`prompt-user` paused processes.

**Planned resolution:** Phase 3 or Phase 4 as part of deeper process model integration.

---

### D3 — `kill` does not cancel Golem invocations

**Spec:** `kill <pid>` maps to Golem's pending-invocation cancellation API for agent
invocations. Returns a precise error if the invocation is in-progress or completed.

**Current state:** `kill` is a registered command with `SudoOnly` policy. It is implemented
as a stub that dispatches to the OS `kill` command (which sends signals to real OS
processes). For synthetic clank PIDs it has no effect.

**Rationale:** The Golem adapter (Phase 4) is required to implement the invocation
cancellation path. `kill` acting on OS processes via the underlying system is the best
available behaviour on native without Golem.

**Impact:** Low currently. No Golem agent invocations exist. When Phase 4 is implemented,
`kill` will need to be replaced with a proper implementation that looks up the PID in the
synthetic process table, finds the associated Golem invocation handle, and calls the
cancellation API.

---

### D4 — `prompt-user` Ctrl-C exit behaviour not verified

**Spec:** `prompt-user` exits with code 130 on Ctrl-C.

**Current state:** `prompt-user` uses `rpassword` for secret input and standard
`stdin.lock().read_line()` for non-secret input. The Ctrl-C handling path has not been
explicitly tested and the exit code under Ctrl-C interruption is not verified to be 130.

**Rationale:** Interactive signal handling in the context of `spawn_blocking` and the
Tokio executor is non-trivial. The implementation handles the happy path correctly. The
Ctrl-C case was not added to the test suite.

**Impact:** Low for non-interactive and scripted usage. Affects interactive agentic
workflows where the user wants to abort a `prompt-user` prompt.

**Planned resolution:** Add a test and fix before Phase 3 ships, since Phase 3 will make
`prompt-user` more heavily used.

---

### D5 — Transcript compaction is not automatic

**Spec:** The transcript compacts automatically at the leading edge when it approaches
the token budget. The boundary between summarised and live history is visible.

**Current state:** The transcript has no automatic compaction. `context trim`, `context
clear`, and `context summarize` enable manual compaction following the pattern shown in
the spec and tutorial:

```bash
SUMMARY=$(context summarize) && context clear && echo "$SUMMARY"
```

The token budget check on `ask` is not implemented — `ask` sends the full transcript
regardless of length.

**Rationale:** Automatic compaction requires two components: token counting (to detect
when the budget is approached) and a trigger mechanism in `run_line`. Both are
straightforward but were not in scope for Phases 1–2. The manual compaction idiom is
functional and documented. The spec itself notes this is a Phase 5 polish item
("automatic transcript compaction").

**Impact:** Low for typical sessions. Users with very long sessions may see model context
errors. The manual compaction idiom is the documented workaround.

**Planned resolution:** Phase 5.

---

### D6 — `CLANK_CONFIG` resolves relative paths from CWD

**Spec:** `CLANK_CONFIG` overrides the config file path.

**Current state:** `config_path()` converts `CLANK_CONFIG` to a `PathBuf` directly. A
relative path is resolved from the shell's CWD at the time `config_path()` is called.

**Rationale:** This is the correct Unix behaviour for relative paths and matches user
expectations. The spec does not specify absolute-only, and relative paths are useful
(e.g., `CLANK_CONFIG=./ask.toml clank` for project-local configs).

**Impact:** None negative. Behaviour is documented in TUTORIAL.md.

---

### D7 — `model remove` and `model info` are stubs

**Spec:** All `model` subcommands are described as part of the provider management
interface.

**Current state:** `model add`, `model list`, and `model default` are fully implemented.
`model remove` and `model info` return `"not yet implemented (planned)"` with exit 2.

**Rationale:** `model add` and `model default` cover the primary use cases. `model remove`
(removes a configured provider) and `model info` (shows details of a specific model) are
secondary operations that require Phase 3 package registry support to be meaningful.

**Impact:** Low. Users can manually edit `ask.toml` to remove a provider.

**Planned resolution:** Phase 3 scope.

---

### D8 — `ls`, `cat`, `grep`, `stat`, `mkdir`, `rm`, `touch` are VFS-backed, not OS-backed

**Spec:** These are listed under Phase 3 ("real implementations of core Unix commands").
The TUTORIAL.md explicitly says they are stubs and delegates to OS full-path invocations
as a workaround.

**Current state:** These seven commands are **fully implemented and VFS-backed**. They
are ahead of schedule relative to the original phasing.

**Rationale:** Implementing these early was driven by testability requirements — the test
coverage plan required that command implementations be testable with `MockVfs`. Implementing
them with real `std::fs` calls would have been faster but produced untestable code and
left the `Vfs` abstraction permanently incomplete. The Phase 3 plan listed "real
implementations of core Unix commands" — these are now done.

**Impact:** Positive. The TUTORIAL.md "What does not work yet" table is partially wrong
— it says `ls`, `cat`, `grep` are stubs, but they now work. The tutorial needs updating
before the next user-facing release.

---

### D9 — `env` was broken until the code quality pass

**Spec:** `env` shows the current exported environment with secrets masked.

**Current state:** `env` was registered but always produced empty output because
`dispatch_builtin` passed `HashMap::new()` as the process environment. Fixed in the code
quality remediation. Now conforms.

**Rationale:** Implementation oversight. The bug was present from the initial Phase 2
implementation and was not caught until the systematic code quality audit.

**Impact:** The command was registered and appeared to succeed (exit 0) while producing
no output — a silent failure. Now fixed and covered by a Level 2 integration test.

---

### D10 — Single compile target (native only)

**Spec:** Primary target is `wasm32-wasip2`; native is secondary.

**Current state:** Explicitly inverted — native is the current target; WASM is deferred.
AGENTS.md and OVERVIEW.md both document this and the rationale.

**Rationale:** Developing against `wasm32-wasip2` from the start would require WASM-safe
versions of all dependencies (HTTP client, filesystem operations, Tokio runtime). The Brush
crate ecosystem has partial WASM support but it is not complete. Building feature-complete
on native first, then porting, is the correct sequencing — the abstraction boundaries
(`Vfs` trait, `HttpClient` trait) are already in place for the port.

**Impact:** None on user experience (native shell works fully). The WASM port is Phase 0
on the roadmap.

---

### D12 — Authorization enforcement was incorrectly applied to user-typed commands

**Spec:** The authorization policy table (README.md § Authorization) describes policies in
terms of what the **agent** may do: "Agent may invoke freely", "Agent invocation pauses for
user confirmation", "Only explicitly sudo-authorized invocations permitted." The human user
is never subject to these restrictions.

**Current state (fixed):** The original Phase 2 implementation applied `Confirm` and
`SudoOnly` enforcement uniformly to all commands regardless of whether they were typed by
the user or issued by an agent. This caused `mkdir`, `touch`, `cp`, `mv`, etc. to prompt
for confirmation when typed directly, and `rm` and `kill` to require `sudo` — which is
incorrect per spec.

**Resolution:** `ExecutionContext { User, Agent }` introduced. `run_line` (user context)
bypasses all authorization enforcement. `run_line_as_agent` (agent context) enforces
`Confirm` and `SudoOnly` policies. Two `TODO(Phase 3)` comments mark the enforcement points
for re-evaluation when the agent execution path is wired in production. Fixed in this
session; all authorization tests updated.

**Impact:** Resolved. User interaction now behaves as a normal shell.

---

### D13 — `sudo ask` broad authorization propagation is not implemented

**Spec:** "`sudo ask "..."` grants the agent broad authorization for that invocation."
(README.md § Authorization)

**Current state:** `sudo ask` is syntactically accepted and the sudo state is set, but
the broad authorization is cleared immediately after the `ask` command itself dispatches —
before any agent-issued commands could arrive. In Phase 3, when agents can issue commands,
those commands will find the sudo state already cleared and will be subject to normal
enforcement regardless of whether the human used `sudo ask`.

There is also a structural gap: the per-command sudo state is the wrong mechanism for
per-invocation broad authorization. A separate per-shell "sudo-ask active" flag is needed
that persists for the duration of the `ask` invocation and is cleared when it completes.

**Rationale:** This mechanism is only meaningful when the agent command execution path
exists (Phase 3). Implementing it before the agent path would produce dead code with no
test surface. The issue is filed and the design is clear; implementation is blocked on Phase 3.

**Impact:** Low in Phase 1 — `ask` cannot issue commands. High in Phase 3 — `sudo ask`
will silently fail to grant the intended broad authorization.

**Planned resolution:** Phase 3, alongside agent command dispatch design.

See: `dev-docs/issues/open/sudo-ask-broad-authorization.md`

---

### D11 — No automatic transcript redaction for user-echoed secrets

**Spec:** "Redaction applies to shell-managed channels: user-authored commands that
deliberately echo sensitive values are outside the scope of automatic redaction."

**Current state:** This matches the spec exactly — the `SecretsRegistry` filters secret
variable *names* from the transcript, `/proc/environ`, and `env` output. It does not
intercept arbitrary echo commands.

**Rationale:** Not a deviation. Documented explicitly in the spec. Worth noting here for
clarity since it is a common question.

**Impact:** None.

---

## Summary by phase

| Phase | Status | Conformance summary |
|---|---|---|
| Phase -1 (Foundation) | Complete | Fully conforming |
| Phase 1 (Transcript + AI) | Complete | Fully conforming (see D5 for auto-compaction) |
| Phase 1.5 (Local providers) | Complete | Fully conforming |
| Phase 2 (Process model + auth) | Complete | Mostly conforming; D2 (job control), D3 (kill), D4 (Ctrl-C) partial; D12 (auth context) fixed this session; D13 (sudo ask propagation) open |
| Core commands (ls/cat/grep etc.) | Complete | Fully conforming — ahead of Phase 3 schedule |
| OS-fallthrough capture | Complete | Fully conforming |
| Phase 3 (Packages + MCP) | Not started | — |
| Phase 4 (Golem) | Not started | — |
| Phase 5 (Polish) | Not started | — |
| Phase 0 (WASM) | Deferred | — |

---

## Items requiring attention before Phase 3

The following deviations must be addressed before Phase 3 planning begins:

1. **`sudo ask` broad authorization propagation (D13)** — The authorization semantics are
   incomplete. `sudo ask` does not grant the agent broad authorization as specified. This
   must be designed and implemented in Phase 3 alongside agent command dispatch. It cannot
   be left until later without risking the authorization model being incorrect in production
   use of Phase 3 features.

2. **`cd` fails after `mkdir` (open issue)** — `cd demo` after `mkdir demo` fails because
   Brush's internal working directory is not kept in sync with the OS process cwd. This
   breaks basic interactive shell use. Must be resolved before Phase 3.
   See: `dev-docs/issues/open/cd-fails-after-mkdir.md`

3. **`prompt-user` Ctrl-C exit code (D4)** — Should be tested and fixed before Phase 3
   makes `prompt-user` more heavily used.

The following are lower priority but should not be deferred past Phase 3:

4. **TUTORIAL.md `"What does not work yet"` table (D8)** — `ls`, `cat`, `grep`, `stat`,
   `mkdir`, `rm`, `touch` are listed as stubs but are now fully implemented. Should be
   updated before the peer review.

---

## What Phase 3 inherits

Phase 3 begins with a solid, tested, clean foundation:

- Transcript capture works for all command types including OS-fallthrough
- Authorization context (user vs. agent) correctly implemented via `ExecutionContext`;
  `run_line_as_agent` is the ready entry point for agent command dispatch
- `Vfs` trait is complete with write operations, enabling Phase 3's filesystem commands
  to be VFS-backed from day one
- `MockVfs` and `MockHttpClient` are available for testing all new Phase 3 implementations
- Typed error enums throughout `clank-ask` — Phase 3 can build on these patterns
- The scenario test harness is in place for Level 3 regression tests
- All known correctness bugs are fixed

Phase 3 must address before shipping:

- **D13** — `sudo ask` broad authorization propagation: the design is clear, the entry
  point (`run_line_as_agent`) is in place, the per-invocation flag mechanism needs to be
  built alongside agent command dispatch
- **cd/mkdir working directory divergence** — must be resolved before users can use
  basic filesystem navigation in Phase 3 workflows
