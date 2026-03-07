---
title: No runnable shell exists — need a native hello-world scaffold
date: 2026-03-07
author: agent
---

## Problem

The clank.sh repository contains a complete design specification but no source code. There is no Rust workspace, no Cargo.toml, and no executable. The project cannot be built, run, or tested in any form.

Before any feature work can begin — transcript management, `ask`, builtins, MCP, Golem — there must be a runnable native binary that proves the foundational dependency (Brush) embeds correctly and that a basic interactive shell loop works.

## Capability Gap

There is no entry point. A developer cannot clone this repo and run anything.

## Why This Matters

All subsequent work depends on this scaffold:

- The internal async process trait has nowhere to live without a crate structure.
- Brush embedding assumptions (API shape, feature flags, `nix` crate exclusion) are unvalidated.
- The dual-target compile story (native + `wasm32-wasip2`) cannot be de-risked until there is something to compile.

## Acceptance Condition

A developer can clone the repo, run `cargo run`, type `echo hello`, and see `hello` printed. The binary exits cleanly on `exit` or Ctrl-D. Nothing more.

## Out of Scope

- WASM / `wasm32-wasip2` target (deferred)
- Transcript
- `ask` or any AI integration
- Custom builtins
- Authorization
- MCP, Golem, `grease`
