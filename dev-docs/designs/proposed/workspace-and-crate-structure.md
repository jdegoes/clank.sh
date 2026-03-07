---
title: "Workspace and Crate Structure"
date: 2026-03-06
author: agent
---

# Workspace and Crate Structure

## Overview

This design specifies the initial Cargo workspace layout and crate decomposition for clank.sh.
The goal is a structure that enforces the layering constraints in the README (no `#[cfg]` at call
sites, all target-specific code at abstraction boundaries), supports the dual compile targets
(`wasm32-wasip2` and native), and provides clean seams for the features added in each phase.

## Workspace Layout

```
clank.sh/
├── Cargo.toml              # workspace root
├── Cargo.lock
├── rust-toolchain.toml     # pin Rust toolchain version
├── crates/
│   ├── clank/              # top-level binary crate (the shell executable)
│   ├── clank-shell/        # core shell logic: transcript, process table, builtins, command dispatch
│   ├── clank-http/         # HttpClient trait + NativeHttpClient + WasiHttpClient
│   ├── clank-vfs/          # VFS trait + handlers (/proc/, /bin/, /mnt/mcp/)
│   ├── clank-ask/          # `ask` subprocess implementation
│   ├── clank-manifest/     # command manifest schema, parsing, and registration
│   ├── clank-golem/        # GolemAdapter trait + NativeGolemAdapter + WasmGolemAdapter
│   └── clank-grease/       # package manager implementation
└── .cargo/
    └── config.toml         # target-specific build configuration
```

## Crate Responsibilities

### `clank` (binary)

The shell binary entry point. Minimal code: wires up dependency injection (constructs
`Arc<dyn HttpClient>`, `Arc<dyn GolemAdapter>`, `Arc<dyn Vfs>`), initializes the terminal layer,
and starts the shell main loop from `clank-shell`. Has separate `main.rs` for native and
`lib.rs` + WIT-exported functions for WASM.

Dependencies: all other crates. No business logic.

### `clank-shell`

The heart of the implementation. Contains:
- Transcript type (sliding window, append, redact, compaction)
- Internal process table (PID tracking, state machine, PPID, job control)
- Brush integration: embeds `brush-parser` and `brush-core`, registers builtins, drives the
  interactive loop
- Builtin implementations: all `parent-shell` and `shell-internal` builtins (`cd`, `export`,
  `context`, `prompt-user`, `jobs`, `fg`, `bg`, `wait`, `type`, `which`, `history`, `alias`,
  `read`, etc.)
- Core command implementations: `ls`, `cat`, `grep`, `ps`, `kill`, `stat`, etc. (all routed
  through `clank-vfs` for path resolution)
- Authorization model: reads `authorization-policy` from manifests, enforces `confirm`/`sudo-only`
- Terminal abstraction: basic stdin/stdout (WASM), readline (native)

Does **not** depend on `clank-ask`, `clank-golem`, or `clank-grease` directly — those are injected
as trait objects or registered as subcommand implementations via `clank-manifest`.

### `clank-http`

Target-varying HTTP client abstraction. Contains:
- `HttpClient` trait (`async fn send(&self, req: Request) -> Result<Response, HttpError>`)
- `HttpError` enum (ConnectionFailed, Timeout, NonSuccessResponse { status, body }, Tls)
- `NativeHttpClient` wrapping `reqwest` — `#[cfg(not(target_arch = "wasm32"))]`
- `WasiHttpClient` wrapping `wstd` — `#[cfg(target_arch = "wasm32")]`
- Logging wrapper: all calls log to `/var/log/http.log` via `clank-vfs` (injected)

No other crate may call `reqwest` or `wstd` directly. All outbound HTTP goes through this crate.

### `clank-vfs`

Virtual filesystem abstraction. Contains:
- `Vfs` trait: `read_file`, `read_dir`, `stat`, `exists`
- `LayeredVfs`: checks a mount table first, then delegates to `std::fs`
- `ProcHandler`: serves `/proc/` from the live process table (injected as `Arc`)
- `BinHandler`: serves `/bin/` as a virtual read-only namespace
- `McpResourceHandler`: serves `/mnt/mcp/<server>/`; static resources from real files, dynamic
  resources via `HttpClient` call to MCP server `resources/read`

No dependency on `clank-shell` (would be circular). `ProcHandler` is given a read-only view of
the process table, not the table itself.

### `clank-ask`

The `ask` subprocess implementation. Contains:
- Transcript window serialization to model API format
- System prompt assembly (initial static version; later dynamic via installed manifests)
- Provider implementations (Anthropic, OpenAI, etc.) using `clank-http`
- `ask repl` REPL loop with isolated transcript
- `model` command implementation

Depends on `clank-http`. Does not depend on `clank-shell` (injected as trait: receives transcript
window as a value, not a mutable reference).

### `clank-manifest`

Command manifest schema and registry. Contains:
- `CommandManifest` type and all its fields (`name`, `synopsis`, `execution-scope`,
  `input-schema`, `output-schema`, `authorization-policy`, `redaction-rules`, `help-text`,
  `subcommands`)
- Manifest registry: a queryable store of all registered manifests, used by tab completion,
  `type`, `which`, `man`, provider tool packaging, and authorization
- Manifest derivation: from Brush builtin registration, from MCP `inputSchema`, from prompt YAML
  frontmatter, from Golem reflected metadata

All crates that register commands depend on this crate.

### `clank-golem`

Golem integration. Contains:
- `GolemAdapter` trait (rollback, fork, oplog, agent introspection, agent invocation)
- `NativeGolemAdapter`: calls Golem HTTP API or returns informative errors — `#[cfg(not(...))]`
- `WasmGolemAdapter`: calls Golem host WIT functions — `#[cfg(target_arch = "wasm32")]`
- `golem` command implementation
- Agent executable generation (from Golem reflected metadata)

Depends on `clank-http` (for native Golem HTTP API calls).

### `clank-grease`

Package manager. Contains:
- Package type definitions (six types)
- Install/remove/list/search/update/info logic
- Local package loader (Phase 3); signed registry client (Phase 5)
- `grease` command implementation
- Package-to-manifest derivation (delegates to `clank-manifest`)

## Dependency Graph

```
clank (binary)
  ├── clank-shell
  │     ├── clank-vfs
  │     ├── clank-manifest
  │     └── brush-core, brush-parser, brush-builtins (external)
  ├── clank-http
  ├── clank-ask
  │     ├── clank-http
  │     └── clank-manifest
  ├── clank-golem
  │     └── clank-http
  └── clank-grease
        ├── clank-http
        ├── clank-manifest
        └── clank-vfs

clank-vfs
  └── clank-http (for McpResourceHandler dynamic resources)

clank-manifest
  (no internal dependencies)
```

No cycles. `clank-shell` does not depend on `clank-ask`, `clank-golem`, or `clank-grease` —
those are injected at startup by `clank` (the binary).

## Target Configuration

`rust-toolchain.toml` pins a specific nightly (required for `wasm32-wasip2` component model async
support) or the latest stable that supports the target, whichever is newer.

`.cargo/config.toml`:
```toml
[build]
# Default to native; override with --target wasm32-wasip2
target = "x86_64-unknown-linux-gnu"  # or detected host

[target.wasm32-wasip2]
# Any wasm32-wasip2-specific linker or runner config
```

## Phase Mapping

| Phase | Crates introduced or significantly extended |
|---|---|
| -1 | `clank`, `clank-shell` (Brush integration, process trait stub), `clank-http` (native only), `clank-vfs` (stub) — native only |
| 0 | `clank-http` (`WasiHttpClient`, `#[cfg]` guards), `clank` (WASM entry point) — WASM portability |
| 1 | `clank-shell` (transcript, terminal), `clank-ask`, `clank-manifest` (initial) |
| 2 | `clank-shell` (process table, job control, auth, `prompt-user`), `clank-vfs` (`ProcHandler`) |
| 3 | `clank-grease`, `clank-manifest` (full), `clank-vfs` (`McpResourceHandler`) |
| 4 | `clank-golem`, `clank-ask` (`ask repl`) |
| 5 | All crates (polish, signed registry, compaction, TUI, man pages) |

## Alternatives Considered

**Single monolithic crate:** Simpler initially but makes the layering constraints unenforceable at
compile time and creates circular dependency risks as features grow.

**Feature flags instead of separate crates for target variants:** Worse ergonomics than separate
crates with `#[cfg]` guards, and does not provide the same compile-time guarantee that call sites
cannot import target-specific types.

**`clank-shell` owning `ask` directly:** Would create a dependency cycle once `ask` needs to
inject a transcript read-only view back into itself for `ask repl`. Separate crate is cleaner.
