---
title: No model command — cannot configure a provider or API key for ask
date: 2026-03-07
author: agent
---

## Summary

`ask` cannot be implemented without knowing which model to call and how to
authenticate with it. The `model` command is the configuration interface for
model providers and API keys. Without it, there is no way to tell clank which
AI model to use or how to reach it.

## Problem

There is no `model` command. There is no configuration file. There is no way
to store or retrieve a provider API key. `ask` has nowhere to look up "which
model should I call?" or "what API key should I use?".

## What the README Requires

From the README:

```
model add anthropic --key $KEY
model remove anthropic
model default sonnet-4.6
model list
model info sonnet-4.6
```

> "`model default` updates `~/.config/ask/ask.toml`. Provider API keys are
> stored in `~/.config/ask/ask.toml` on native or in Golem's secrets API when
> running inside Golem."

Provider notation is `provider/model`. Unambiguous model names can omit the
provider prefix (e.g. `sonnet-4.6` resolves to `anthropic/claude-sonnet-4-5`).

## Scope for MVP

For the MVP, only the commands `ask` strictly requires are needed:

- `model add <provider> --key <key>` — register a provider and its API key
- `model default <model>` — set the default model used by `ask`
- `model list` — show configured providers and the current default

`model remove` and `model info` are useful but not blocking `ask`. They are
deferred.

## Configuration Storage

`~/.config/ask/ask.toml` on native. The file must be readable by `ask` to
retrieve the configured provider, model, and API key.

For MVP, a simple TOML structure is sufficient:

```toml
default_model = "anthropic/claude-sonnet-4-5"

[providers.anthropic]
api_key = "sk-ant-..."
```

## Execution Scope

`model` is `shell-internal` scoped per the README — it operates on shell
configuration (a persistent file), not as a subprocess. It is intercepted in
the REPL directly, consistent with how `context` was implemented.

## Acceptance Condition

- `model add anthropic --key $KEY` writes the key to `~/.config/ask/ask.toml`.
- `model default sonnet-4.6` sets the default model in the config file.
- `model list` prints configured providers and the current default.
- `ask` can read the config file to determine which model and key to use.
- All existing tests continue to pass.
