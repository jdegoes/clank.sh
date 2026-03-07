---
title: "Golden test infrastructure is fragile and incomplete"
date: 2026-03-06
author: agent
---

# Golden test infrastructure is fragile and incomplete

## Problem

The current golden test infrastructure (`trycmd` + `.toml` descriptors + `.stdout`/`.stderr`
sidecar files) has accumulated several concrete deficiencies as the feature set has grown:

1. **No config isolation for the `ask/` fixtures.** `ask_no_config.toml`,
   `context_clear.toml`, `context_show_empty.toml`, and `model_list_no_config.toml` do not
   set `CLANK_CONFIG`. They run against whatever `~/.config/ask/ask.toml` exists on the
   developer's machine. On a configured machine the output changes and the tests give false
   results.

2. **Four fixtures assert nothing about output.** The same four `ask/` fixtures have no
   `.stdout` or `.stderr` sidecar files. trycmd only checks exit code. They provide zero
   regression protection for the commands they nominally cover.

3. **Test cases are split across multiple files.** A single scenario requires a `.toml`
   descriptor, a `.stdout` file, a `.stderr` file, and potentially `.in/` and `.out/`
   directories. The full picture of a test case is never visible in one place.

4. **No structured assertions on resulting filesystem state.** After `model add`, the only
   way to assert that the config file was written correctly is via `.out/` directories, which
   trycmd requires to be exact byte-for-byte matches of the entire sandbox. There is no way
   to assert on specific fields within a written TOML file.

5. **The format does not extend cleanly to Phase 3 and beyond.** Phase 3 introduces `grease
   install`, MCP session management, and VFS mounts. Each of these involves pre-populated
   filesystem state, command sequences, and post-command filesystem assertions. The trycmd
   split-file model becomes progressively more unwieldy as this surface grows.

6. **`ask.rs` and `shell_basics.rs` system tests have no config isolation either.** The
   `assert_cmd`-based tests in these files do not set `CLANK_CONFIG`, so they also run
   against the developer's real config.

## Capability Gap

- No single-file test format that captures input state, command sequence, expected output,
  and expected resulting state together.
- No built-in, always-on config isolation (every test must manually handle `CLANK_CONFIG`).
- No structured way to assert on config file contents after a command.
- No way to pre-populate config state as input to a test.

## Out of Scope

Changes to Level 1 (unit) or Level 2 (crate integration) tests. This issue is exclusively
about the Level 3 golden/scenario test infrastructure.
