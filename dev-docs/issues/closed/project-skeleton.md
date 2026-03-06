---
title: "No project skeleton — codebase is unbuildable"
date: 2026-03-06
author: agent
---

## Problem

The repository contains no Rust source code, no `Cargo.toml`, and no workspace structure. Nothing
can be compiled or tested. There is no entry point, no crate graph, and no build configuration.

## Impact

All subsequent implementation work is blocked. Without a buildable skeleton, no code can be
written, tested, or iterated on.

## Desired Outcome

A minimal Rust workspace that:

- Compiles on a native target (`cargo build` exits 0)
- Has a passing test suite (`cargo test` exits 0)
- Runs as an executable that accepts shell input and produces correct output
- Establishes the crate decomposition called out in the README architecture

The skeleton need not be feature-complete. It exists solely to unblock all further development.
