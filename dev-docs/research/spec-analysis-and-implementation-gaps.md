---
title: "Spec Analysis and Implementation Gaps"
date: 2026-03-06
author: agent
---

# Spec Analysis and Implementation Gaps

## Motivation

Before writing implementation plans, a full read of `README.md` was conducted to assess how
complete and actionable the requirements are, to identify gaps that will require design decisions
before or during implementation, and to propose a phased implementation order.

## What Is Well-Specified

The README is unusually complete for a pre-implementation project. The following areas are
specified in enough detail to implement directly without further design work:

**Command surface and execution scopes**
Every builtin is named with its `execution-scope` (`parent-shell`, `shell-internal`, `subprocess`).
Special builtins (`cd`, `exec`, `exit`, `export`, `source`, `unset`) are distinguished from
ordinary builtins. Core commands (`ls`, `grep`, `jq`, `ps`, `kill`, etc.) are enumerated. The
AI/platform commands (`ask`, `model`, `mcp`, `golem`, `grease`, `context`, `prompt-user`) are
fully described with their flags and semantics.

**Process model**
Process states (`R`, `S`, `T`, `Z`, `P`), PID semantics (monotonically increasing, never reused,
not valid across forks), PPID tracking, `/proc/<pid>/` layout (`cmdline`, `status`, `environ`),
`ps aux`/`ps -ef` column format, and job control (`&`, `jobs`, `fg`, `bg`, `wait`) are all
specified. The `P` state (paused, awaiting human) is first-class.

**Authorization model**
Exactly three levels (`allow`, `confirm`, `sudo-only`). Per-command in manifest. Table of which
operations get which policy. `sudo` means conscious human authorization, not Unix credentials.
Agents cannot use `sudo`; they pause and surface a confirmation request. `sudo ask` grants broad
authorization for that invocation.

**Filesystem layout**
Full directory tree specified, including virtual namespaces (`/proc/`, `/bin/`, `/mnt/mcp/`).
Default `$PATH` specified. Shadowing behavior across directories is intentional and specified.
`/dev/null`, `/dev/stdin`, `/dev/stdout`, `/dev/stderr` supported.

**Transcript mechanics**
Sliding window. Compaction replaces oldest portion with a visible summary block. `context` builtin
subcommands (`show`, `clear`, `summarize`, `trim`) and their non-re-recording contract. `ask repl`
transcript inheritance modes (`--fresh`, `--inherit`, default = summary injection). Redaction rules
apply at all times; `--secret` responses never enter transcript.

**`ask` command**
`--json` contract (valid JSON on stdout or exit `6`, raw response to stderr). `--fresh`,
`--no-transcript`, `--inherit` flags. Stdin supplementation (transcript first, stdin appended as
separate channel). `sudo ask` semantics. Tool surface = `subprocess`-scoped commands +
skills; `parent-shell` and `shell-internal` excluded except `prompt-user`.

**`prompt-user` builtin**
Markdown on stdin. `--choices`, `--confirm`, `--secret` flags. Process enters `P` state while
awaiting. Exit `0` on response, `130` on Ctrl-C. `--secret` responses never enter transcript,
logs, or completion caches.

**MCP integration**
HTTPS-only (no stdio, deliberate). Session lifecycle (`mcp session list/open/close/info`). Four
resource types (static, dynamic, binary, templates). Resource templates as executables. `mcp watch`
for subscriptions. MCP resources mounted under `/mnt/mcp/<server>/`. Tools as subcommands under
server name in `/usr/lib/mcp/bin/`. Prompts in `/usr/lib/prompts/bin/`.

**Golem agent CLI grammar**
`<agent> [<constructor-flags>] [<wrapper-flags>] <method> [--] [<method-args>]`. Reserved wrapper
flags (`--revision`, `--phantom`, `--trigger`, `--schedule`). Reserved subcommands
(`oplog`, `stream`, `repl`, `status`, `help`). Ephemeral vs durable types (same grammar, different
reserved subcommand availability). Upsert invocation model: no `new` subcommand on installed
executable. `/proc/<pid>/status` Golem extension fields specified.

**Package system (`grease`)**
Six package types: standalone prompts, MCP server artifacts (tools/prompts/resources), Golem agent
types, shell scripts, skills, future wRPC components. Content-addressed, signed. Every package must
provide enough metadata to produce a command manifest or is rejected at install time.

**Exit codes**
Fully specified (0-7, 126, 127, 130). Meaningful across all process types. `&&`/`||` chaining
works correctly across all process types.

**Logging**
Three layers: human-readable `/var/log/` files (`shell.log`, `http.log`, `mcp.log`, `ops.log`),
structured audit events (machine-readable, PID/PPID addressable), Golem oplog (not a file).

**Brush integration**
Uses `brush-parser` + `brush-core` + selectively `brush-builtins`. Replaces `brush-interactive`
with transcript-aware layer. Replaces entire Unix process spawning model with internal async process
trait. Known gaps inherited from Brush: `coproc`, `select`, `ERR` traps, some `set`/`shopt` flags.

**Compile targets**
`wasm32-wasip2` (primary) and native (secondary). HTTP seam: `reqwest` on native, `wstd` on WASM,
both behind `HttpClient` trait in `clank-http`. `nix` crate excluded at process trait boundary.
No `#[cfg]` at call sites.

## Identified Gaps

These areas require design decisions before or during implementation.

### Gap 1: System prompt content

The README explicitly defers the system prompt as "a prompt engineering problem whose solution will
evolve." The shape is clear: it tells the model where it is, maps the filesystem layout, describes
available tools and their semantics, and explains `prompt-user`. The actual content and structure
need to be drafted as a separate design artifact and iterated on. The virtual file
`/proc/clank/system-prompt` (computed on read from installed tools and config) means the system
prompt is dynamic — a design for how it is assembled programmatically is needed.

### Gap 2: Transcript compaction algorithm

The mechanism is described (summarize leading edge, replace with visible block) but the following
are unspecified:
- Triggering condition: token count threshold? configurable?
- Which model is used to produce the summary?
- What is the default token budget?
- What knobs are exposed in config?
- How is the summary block rendered in the terminal vs stored in the transcript?

### Gap 3: Tab completion implementation

Tab completion is mentioned throughout as a first-class feature (manifest-driven, covers all
commands including agent constructor flags and method names). How Brush's completion layer gets
replaced or extended is not specified. Brush exposes `brush-core`'s extension API for builtins;
whether it has a comparable hook for completion is unknown and requires research.

### Gap 4: `grease` registry protocol

"Signed, content-addressed" packages are required (non-negotiable per README). The registry API
format, package manifest schema, signing scheme (which key infrastructure?), and transport are
entirely unspecified. This is a significant design surface and likely a Phase 5 concern, but the
install-time package format needs enough definition to support local packages from Phase 3 onward.

### Gap 5: Virtual filesystem driver

`/proc/`, `/mnt/mcp/`, and `/bin/` are virtual read-only namespaces. The README specifies "no
FUSE" and "implemented at the shell level." Brush operates against the host filesystem via the Rust
standard library. The mechanism for intercepting filesystem operations and routing them to virtual
handlers — without FUSE, inside a WASM component — is unspecified and has significant design
implications. Needs a research spike before Phase 2 (where `/proc/` is needed) and Phase 3 (where
`/mnt/mcp/` is needed).

See companion research doc: `dev-docs/research/virtual-filesystem-driver-options.md`.

### Gap 6: TTY / terminal abstraction

Described as "a Rust abstraction for basic stdin/stdout (WASI-compatible) and a separate one for
full TUI." The README says the native implementation defines the target experience and the WASM
target degrades to stdin/stdout until Golem adds TTY extensions. No library choice, abstraction
interface, or degradation contract is specified. `Ctrl-Z` (SIGTSTP) is explicitly deferred to
native-only in v1.

### Gap 7: Audit event schema

"Machine-readable, addressable by PID and PPID." Golem invocation entries include agent type,
agent parameters, revision, phantom UUID, and idempotency key. Format (JSON? structured log line?
binary?) and the full field schema are unspecified.

### Gap 8: MCP OIDC/OAuth on native target

"Requires external configuration." No detail on what that configuration looks like, where it
lives, or how the shell reads it. OIDC flows in particular require a redirect URI and a local HTTP
listener or browser handoff — neither of which fits cleanly inside a WASM component. The native
target presumably has more latitude here. Needs design before MCP auth can be fully implemented.

### Gap 9: Golem cluster configuration (native target)

"External to the shell — a concern only for the native binary, living outside the shell's
filesystem." No detail on what "external" means: a config file? environment variables? the Golem
CLI's own config? This is required before any Golem features work on native.

### Gap 10: In-WASM concurrency mechanics

"Multiple shell processes can make progress concurrently via Golem's Wasmtime runtime, which has
component-model async concurrency enabled." The mechanism by which multiple synthetic processes
(each an impl of the internal `Process` trait) are driven concurrently within a single WASM
component is unspecified. Whether this uses Rust async tasks, a hand-rolled executor, or Wasmtime's
built-in async support needs to be understood before the process model is implemented.

## Gap Prioritization

| Gap | Phase Needed | Priority |
|---|---|---|
| Virtual filesystem driver (Gap 5) | Phase 2 | Critical — blocks `/proc/` |
| In-WASM concurrency mechanics (Gap 10) | Phase 0/1 | Critical — affects process trait design |
| Tab completion (Gap 3) | Phase 3 | High — core UX |
| System prompt content (Gap 1) | Phase 1 | High — `ask` barely usable without it |
| Transcript compaction (Gap 2) | Phase 1/5 | Medium — needed before token budget is hit |
| TTY / terminal abstraction (Gap 6) | Phase 0 | Medium — needed from day one, but degrades gracefully |
| Golem cluster config (Gap 9) | Phase 4 | Medium — blocks native Golem features |
| Audit event schema (Gap 7) | Phase 4/5 | Low — logging works without structured events |
| `grease` registry protocol (Gap 4) | Phase 5 | Low — local installs work without signed registry |
| MCP OIDC/OAuth (Gap 8) | Phase 5 | Low — most MCP servers use API key auth |

## Proposed Implementation Phases

### Phase -1 — Native-only foundation
Cargo workspace, Brush integration, internal process trait, native target only. No WASM,
no dual-target abstractions, no CI for WASM. Shell starts and executes basic commands on native.
Validates the core architecture with the tightest possible feedback loop.

### Phase 0 — Add wasm32-wasip2 target
Introduce `WasiHttpClient`, exclude `nix` at the process trait boundary, add WASM component entry
point, CI for both targets. No new user-visible features — portability only.

### Phase 1 — Transcript and `ask`
Transcript as owned value, `context` builtin, `ask` subprocess calling a model via HTTP, response
appended to transcript. `--json`, `--fresh`, `--inherit`. Exit codes enforced. Stdout/stderr
discipline.

### Phase 2 — Process model and job control
Process table, `P` state, `ps`, `jobs`, `fg`, `bg`, `wait`, `kill` (synthetic processes),
background execution, `prompt-user` builtin, authorization model, `/proc/` virtual namespace.

### Phase 3 — Package system and MCP
Command manifest schema, virtual filesystem driver (needed for `/mnt/mcp/`), `grease` (local
packages first), MCP HTTPS transport, session lifecycle, tools/prompts/resources, tab completion
backed by manifests.

### Phase 4 — Golem integration
`GolemAdapter` trait (native HTTP API + WASM host functions), agent executable generation and CLI
grammar, all invocation modes, `golem` command, `ask repl`, durability, structured logging.

### Phase 5 — Polish and production-readiness
Signed `grease` registry, system prompt iteration, transcript compaction, full TUI on native,
`model` command and config, `man` pages, audit events, skills packaging, MCP OIDC/OAuth.

## References

- `README.md` — full project specification
- `dev-docs/research/virtual-filesystem-driver-options.md` — companion research on virtual FS
