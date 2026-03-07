---
title: "Pre-Phase 3 cleanup: three small items from retrospective"
date: 2026-03-07
author: agent
---

# Pre-Phase 3 cleanup

## Problem

The retrospective (`dev-docs/retrospective.md`) identified three items that should be
resolved before Phase 3 planning begins:

1. **TUTORIAL.md is out of date.** Section 12 "What does not work yet" lists `ls`, `cat`,
   `grep` (and others) as stubs, and lists `model add` as in-progress. All of these are
   now fully implemented. The note telling users to use `/bin/ls` as a workaround will
   confuse anyone reading the tutorial.

2. **`model default` is a stub.** `model default <model>` would set the `default_model`
   field in `ask.toml`. The config machinery exists; only the `run_model` dispatch arm and
   a test are missing.

3. **`prompt-user` Ctrl-C exit code is untested.** The implementation correctly returns
   exit 130 on EOF/Ctrl-C — the research confirmed this. But there is no test asserting it,
   so a future regression would be invisible.

## Capability gap

- `model default <model>` prints "not yet implemented" — users have to edit `ask.toml`
  manually to change the default model.
- TUTORIAL.md describes a stale state of the shell that will mislead new users.
- `prompt-user` Ctrl-C contract has no test coverage.

## Out of scope

`model remove` and `model info` remain as stubs — these are lower priority and can wait
for Phase 3 planning.
