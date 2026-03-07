---
title: "Phase 4: Golem integration, agent executables, durability, and `ask repl`"
date: 2026-03-06
author: agent
---

# Phase 4: Golem integration, agent executables, durability, and `ask repl`

## Problem

The shell cannot interact with the Golem cluster. Golem agent types cannot be installed or invoked.
The shell instance is not durable — transcript and filesystem are ephemeral. `ask repl` does not
exist. Structured logging is incomplete.

## Capability Gap

- `GolemAdapter` trait does not exist; Golem-dependent features all fail at the first call.
- `grease install golem:<type>` does not generate agent executables.
- Agent CLI grammar (constructor flags, wrapper flags, methods, reserved subcommands) is not
  implemented.
- Invocation modes (await, fire-and-forget, scheduled) are not implemented.
- `kill` is not wired to Golem invocation cancellation via idempotency key.
- `/proc/<pid>/status` Golem extension fields (`agent-type`, `agent-params`, `agent-revision`,
  `phantom-uuid`, `idempotency-key`) are not populated.
- `golem` command does not exist.
- Durability: transcript and filesystem are not durable component state (WASM only).
- `ask repl` subprocess with isolated transcript does not exist.
- Structured audit events are not emitted.
- `shell.log`, `mcp.log`, `ops.log` are incomplete; `http.log` may be from Phase 1.

## Deliverables

Full Golem integration on both native (via Golem HTTP API) and WASM (via host functions). Durable
agent workflows are possible. `ask repl` works. Logging is complete.

Concretely:
- `GolemAdapter` trait with two implementations:
  - `NativeGolemAdapter`: calls Golem HTTP API (when cluster is configured) or returns informative
    errors (when not configured)
  - `WasmGolemAdapter`: calls Golem host functions directly
- `grease install golem:<type>`: deploys to Golem cluster, generates executable in
  `/usr/lib/agents/bin/` with correct CLI grammar
- Agent executable CLI grammar: constructor flags, wrapper flags (`--revision`, `--phantom`,
  `--trigger`, `--schedule`), method subcommands, `--` parse boundary
- Reserved subcommands: `oplog`, `stream`, `repl`, `status`, `help` (durable only: `oplog`,
  `repl`, `status`)
- Invocation modes: await (default), `--trigger` (fire-and-forget), `--schedule <iso8601>`
- Idempotency keys for all invocation modes
- `kill <pid>` for Golem invocations: maps PID → idempotency key → Golem cancellation API
- `/proc/<pid>/status` Golem extension fields
- `golem agent list/new/oplog/interrupt/resume`
- `golem connect <agent-identity>`: inspect running agent (oplog, files, status)
- `golem oplog`: clank shell instance's own oplog
- `golem rollback` and `golem fork` (WASM target only)
- Golem cluster config for native: spec and load mechanism (to be designed)
- Durability on WASM: transcript and filesystem as durable Golem component state
- `ask repl`: isolated transcript; `[model]>` prompt; `:new-session`, `:model`, `:exit` meta-
  commands; `Ctrl-C` cancels in-flight turn then exits; background-able with `&`; transcript
  inheritance flags (`--fresh`, `--inherit`, default = summary injection)
- Structured audit events: per-process, addressable by PID/PPID, Golem fields for agent
  invocations
- Complete logging: `shell.log`, `http.log`, `mcp.log`, `ops.log`

## Open Questions Requiring Design

- Golem cluster config for native target: file location, format, auth mechanism. (Gap 9 in spec
  analysis research doc.)
- Audit event schema: format (JSON lines?), field set, file vs stream. (Gap 7.)
- `GolemAdapter` host function interface for WASM target: which WIT interfaces are called? Requires
  research against Golem's published WIT definitions.

## Out of Scope

Signed `grease` registry, full TUI / `Ctrl-Z`, MCP OIDC/OAuth, `man` pages, skills packaging,
transcript auto-compaction. All deferred to Phase 5.
