---
title: "Phase 5: Polish and production-readiness"
date: 2026-03-06
author: agent
---

# Phase 5: Polish and production-readiness

## Problem

The shell is functionally complete after Phase 4 but is not production-ready. The `grease`
registry is unsigned and local-only, transcript compaction is manual-only, the TUI is degraded on
native, the system prompt is not fully iterated, and several secondary features are missing or
stubbed.

## Capability Gap

- `grease` installs from local paths only; no signed, content-addressed registry.
- Transcript compaction is manual (user must run `context summarize && context clear`); no
  automatic compaction when the token budget is approached.
- Full TUI features (`Ctrl-Z`, process groups, terminal resize, readline history) work on native
  but the degraded WASM path is minimal.
- System prompt content has not been iterated for quality — the initial version from Phase 1 is a
  placeholder.
- `man` pages are stubs.
- Skills packaging is not fully implemented.
- MCP OIDC/OAuth on native target is not implemented.
- Audit event structured format is finalized but output path may need refinement.
- `/proc/clank/system-prompt` accuracy: does it reflect all installed skills and their reference
  documents?

## Deliverables

A production-ready shell suitable for real use. `grease` can install from a real signed registry.
Transcript compaction is automatic. The TUI is complete on native. The system prompt is
well-engineered.

Concretely:
- Signed `grease` registry: content-addressed package payloads, signed metadata,
  transparency-auditable. Registry API and package format finalized. `grease registry add/list/remove`.
- Transcript auto-compaction: configurable token budget, trigger at threshold, summary model call,
  visible summary block in transcript, configurable knobs in `~/.config/ask/ask.toml` (or
  `/etc/clank/config.toml` for system-wide).
- Full TUI on native: `Ctrl-Z` (SIGTSTP), process groups, terminal resize handling, improved
  readline (multi-line editing, syntax highlighting if feasible).
- System prompt iteration: structured assembly from installed manifests + skills, quality-reviewed
  content, iterative improvement.
- Complete `man` pages for all builtins and core commands.
- Skills packaging: full implementation of reference document indexing and per-skill `bin/`
  directory on `$PATH`.
- MCP OIDC/OAuth on native: external config mechanism, browser handoff or local HTTP listener,
  token storage.
- Audit event output path: file format, rotation, max size, access controls.
- `model` command: complete implementation of `model list/add/remove/default/info`.
- End-to-end acceptance test suite: covers all features across native and WASM targets.

## Open Questions Requiring Design

- Signed registry protocol: which signing scheme? (Sigstore? custom?) Which transparency log?
  What does the package manifest schema look like in full?
- Transcript compaction: which model produces the summary? Same as the configured default, or a
  fixed fast/cheap model? How is the summary block stored vs displayed?
- MCP OIDC/OAuth: how is the redirect URI handled in a WASM/server context? What credential store
  is used on native?

## Out of Scope

wRPC WASM component process type (listed as roadmap in README, not v1). MCP sampling (explicitly
not addressed in v1). Golem TTY extensions (pending upstream Golem work).
