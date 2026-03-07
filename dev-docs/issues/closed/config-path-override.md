---
title: "Allow config file path to be overridden via environment variable"
date: 2026-03-06
author: agent
---

# Allow config file path to be overridden via environment variable

## Problem

The ask config file path is hardcoded to the platform default returned by
`dirs-next::config_dir()` — on macOS this is `~/Library/Application Support/ask/ask.toml`,
on Linux `~/.config/ask/ask.toml`. There is no way to point clank at a different config
file without modifying that path.

## Capability Gap

- Developers cannot keep a project-local `ask.toml` in their working directory.
- CI environments cannot inject a config without writing to the user's home directory.
- Multiple configurations (different API keys, different default models) cannot coexist
  without overwriting the user config.
- Testing `ask` end-to-end with a real API key requires either polluting the user config
  or adding special-case test logic.

## Deliverables

When the environment variable `CLANK_CONFIG` is set to a file path, `ask` (and
`context summarize`) loads config from that path instead of the platform default. If the
variable is set but the file does not exist, the same error is returned as for any missing
config file. If the variable is not set, behaviour is unchanged.
