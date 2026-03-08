---
title: "No buildable Rust workspace exists"
date: 2026-03-06
author: agent
---

# No buildable Rust workspace exists

## Problem

The repository contains only documentation and scaffolding. There is no `Cargo.toml`, no crate structure, and no source code. The project cannot be built, tested, or iterated on by any contributor or agent.

## Impact

All implementation work is blocked. No toolchain, dependency, or compilation assumption can be validated until a minimal workspace exists that successfully compiles for both the native target and `wasm32-wasip2`.

## Context

The design (`README.md`) specifies a Rust workspace targeting `wasm32-wasip2` (primary) and native Rust (secondary). It identifies `brush-core` as the shell interpreter foundation and `reqwest`/`wstd` as the HTTP client backends, abstracted behind a shared trait. The workspace must support both targets from day one because conditional compilation is a load-bearing architectural concern.

No `rust-toolchain.toml`, no `Cargo.toml`, and no `.cargo/config.toml` exist yet.

## Out of Scope

This issue does not cover implementing any shell builtins (`ask`, `grease`, `prompt-user`), the `grease` package manager, MCP integration, Golem-specific durability features, or the AI transcript/context system. Those are separate capability issues to be filed once the workspace is established.
