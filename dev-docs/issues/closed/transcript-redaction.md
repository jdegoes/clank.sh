---
title: "Transcript does not redact sensitive values — secrets, tokens, PII will be sent to the model"
date: 2026-03-07
author: agent
---

# Transcript does not redact sensitive values — secrets, tokens, PII will be sent to the model

## Problem

The transcript currently records command text and captured output verbatim.
There is no redaction of sensitive values before they are stored as
`TranscriptEntry` nodes. When `ask` is implemented and consumes the
transcript as the model's context window, anything that has appeared in a
command or its output will be visible to the model — including passwords,
API keys, tokens, private key material, and any other sensitive data the
user typed or a command printed.

The README is explicit on this:

> Redaction rules apply at all times. Anything governed by a `redaction-rules`
> entry in a command manifest never enters the transcript — not through direct
> output, not through summarization. Secrets do not leak into the AI's view
> of the session.

This must be resolved before `ask` is implemented. Sending an unredacted
transcript to an external model API is the highest-impact security gap in
the current codebase.

## Two redaction mechanisms required

### 1. Heuristic regex redaction (automatic)

The transcript must scan every `Command` and `Output` entry for well-known
sensitive patterns and replace matched values with `[REDACTED]` before
storage. This is a best-effort defence that catches values which were never
explicitly declared secret — an API key accidentally printed by a command,
a password typed as a command argument, a JWT in an HTTP response.

Always-on pattern categories:
- **Config-secret pattern** — fully case-insensitive; matches any identifier
  (env var, TOML key, YAML key) whose name contains `token`, `secret`,
  `password`, `passwd`, `pwd`, `api_key`, `credential`, or `auth_key`
  (any prefix/suffix, any case), in assignment or display form with optional
  quoting. Covers all common config formats:
  - Shell/`.env`: `DB_PASSWORD=hunter2`, `STRIPE_API_TOKEN=sk_live_abc`
  - TOML: `password = "hunter2"`, `api_key = "sk_live_abc"`
  - YAML: `password: hunter2`, `auth_key: "abc123"`
  The identifier is preserved; only the value is replaced with `[REDACTED]`.
- Generic `--key`/`--token`/`--password`/`--secret` CLI flag patterns
- AWS access key IDs (`AKIA[0-9A-Z]{16}`)
- GitHub tokens (`ghp_*`, `github_pat_*`)
- Generic JWTs (three base64url segments starting `eyJ`)
- PEM private key headers (`-----BEGIN * PRIVATE KEY-----`)
- HTTP `Authorization: Bearer <token>`

High-false-positive categories (IPv4, email, credit card numbers) are
intentionally excluded from the default set. They can be added via future
configuration.

### 2. Manifest `redaction-rules` (declarative)

`redaction-rules` is a top-level field in every command manifest, listed
alongside `execution-scope`. It declares which argument names have values
that must never appear in the transcript, `ps`, logs, history, completion
caches, or provider manifests.

This is not a runtime mechanism — it is declared per-command at install time
(or in Rust for builtins). When a command is recorded as a `Command` entry,
the recording call site looks up the manifest, extracts the values of any
declared `redaction-rules` arguments from the command text, and scrubs them
with exact string replacement before storage.

Example: a future `model add --key $API_KEY` command would declare `--key`
in its `redaction-rules`. The `Command` entry would be stored as
`model add --key [REDACTED]`, regardless of whether the key matched any
heuristic pattern.

## What is explicitly out of scope

- `export --secret KEY=value` runtime variable tracking — this is a separate
  mechanism described in the README for marking shell variables as sensitive.
  It is deferred to a later issue and plan.
- Redaction of values that were never routed through shell-managed channels.
  The README is clear: "user-authored commands that deliberately echo sensitive
  values are outside the scope of automatic redaction." `echo mypassword`
  produces `mypassword` — heuristic redaction only catches it if the value
  matches a known pattern.
- Retroactive redaction of entries already in the transcript — redaction
  applies at write time only.
- Redaction in `context show` output — entries are stored already-redacted,
  so display methods output what was stored.
- The `--secret` flag on `prompt-user` responses — a separate mechanism
  handled at the `prompt-user` builtin level.

## Important limitation: heuristic redaction is pattern-based only

Heuristic redaction catches secrets that follow recognisable patterns. The
env-var-name pattern (`*TOKEN*`, `*PASSWORD*`, etc.) covers the common `.env`
file format well: `DB_PASSWORD=hunter2`, `STRIPE_API_TOKEN=sk_live_abc`, and
`MY_APP_SECRET=xyz` are all redacted because their variable names signal
sensitivity.

What heuristic redaction cannot catch are secrets stored in variables with
non-sensitive-sounding names: `cat .env` containing `MYAPP=hunter2` or
`CONF=abc123` would not be redacted — there is no signal in the name. The
full solution for those cases requires `export --secret` variable tracking:
once the shell knows the values upfront it can scrub them from any output.
Without that mechanism, the remaining safeguard is operational discipline
(`context clear` after sessions involving sensitive files).
