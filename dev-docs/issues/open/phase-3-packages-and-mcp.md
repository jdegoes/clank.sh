---
title: "Phase 3: Package system (`grease`), MCP integration, and tab completion"
date: 2026-03-06
author: agent
---

# Phase 3: Package system (`grease`), MCP integration, and tab completion

## Problem

The shell cannot be extended. There is no package manager, no MCP server integration, no way to
install prompts or tools, and no tab completion backed by command manifests. The AI's tool surface
is fixed at the builtins that ship with the shell.

## Capability Gap

- `grease` command does not exist.
- Command manifest schema is not defined.
- MCP HTTPS transport is not implemented.
- MCP session lifecycle (`mcp session` commands) does not exist.
- MCP tools, prompts, and resources are not installable.
- `/mnt/mcp/<server>/` virtual namespace does not exist.
- Tab completion is whatever Brush provides by default — not manifest-driven.
- Skills packaging does not exist.

## Deliverables

The full package install/remove lifecycle works for local packages (no signed registry yet).
MCP HTTPS servers can be installed and their tools invoked. MCP resources are mounted and readable.
Tab completion is driven by command manifests.

Concretely:
- Command manifest schema: `name`, `synopsis`, `execution-scope`, `subcommands`, `input-schema`,
  `output-schema`, `authorization-policy`, `redaction-rules`, `help-text`
- Manifest registration for all existing builtins and core commands
- `grease install/remove/list/search/update/info` — local packages only
- Six package types: standalone prompts, MCP server artifacts
  (tools/prompts/resources, selectable), Golem agent types (stub — deployment deferred to Phase 4),
  shell scripts, skills, future wRPC (stub)
- Parameterized prompt installation: generated shell scripts in `/usr/lib/prompts/bin/`
- Non-parameterized prompt installation: shebang executable
- MCP HTTPS transport via `HttpClient`
- `mcp session list/open/close/info`
- MCP tools → executables in `/usr/lib/mcp/bin/` (server name = command, tools = subcommands)
- MCP prompts → executables in `/usr/lib/prompts/bin/`
- MCP static resources → files in `/mnt/mcp/<server>/`
- MCP dynamic resources → virtual files (fetch `resources/read` on each read)
- MCP binary resources → files with appropriate extensions
- MCP resource templates → executables in `/usr/lib/mcp/bin/`, stubs in `/mnt/mcp/<server>/`
- `mcp watch` for subscriptions
- `mcp resource info <path>` for MCP-specific annotations
- `stat` on mounted resources reflects `lastModified` and resource type
- Tab completion for all installed commands, subcommands, flags (manifest-driven)
- `--help` on every command and installed package executable
- `man` stubs (full `man` pages deferred to Phase 5)
- `/proc/clank/system-prompt` now dynamic: assembled from installed manifests and skills on read

## Open Questions Requiring Design

- Virtual filesystem driver for `/mnt/mcp/` — must be resolved before MCP resources can be
  mounted. See `dev-docs/research/virtual-filesystem-driver-options.md`. A Brush spike on I/O
  hooks is the prerequisite.
- Tab completion extension point in Brush — how does Brush's interactive layer expose a hook for
  manifest-driven completions? Requires research spike against Brush source.
- `grease` local package format — what does a local package look like before the signed registry
  exists? Needs a design doc.

## Out of Scope

Signed `grease` registry (Phase 5). Golem agent executable generation (Phase 4). MCP OIDC/OAuth
(Phase 5). Full `man` pages (Phase 5).
