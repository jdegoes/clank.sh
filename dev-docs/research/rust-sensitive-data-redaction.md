---
title: "Rust crates for sensitive data detection and redaction"
date: 2026-03-07
author: agent
---

# Rust crates for sensitive data detection and redaction

## Question

What Rust crates are available for detecting and redacting sensitive
information (passwords, API keys, tokens, PII, etc.) from arbitrary text
strings, with compatibility for the `wasm32-wasip2` target?

## Conclusion

No single production-ready Rust crate combines a comprehensive secret-pattern
library with confirmed `wasm32-wasip2` compatibility. The best approach is
to use the `regex` crate (which is WASM-compatible) with a curated set of
patterns drawn from well-known tools and embedded directly in
`clank-transcript`. The `regex` crate's `replace_all` method makes the
redaction call site trivial once patterns are compiled.

## Candidate crates

### `regex` (recommended — dependency only)

- Version: 1.x (stable)
- WASM compatibility: confirmed for `wasm32-wasip2` (Tier 2 target, all `std`
  features available; `regex` has no OS-specific or network dependencies)
- Purpose: pattern matching and `replace_all` for redaction
- License: MIT / Apache-2.0
- Patterns must be embedded manually, but this gives full control

### `secretscan` (pattern source, not for direct use)

- Version: 0.2.2
- WASM compatibility: **no** — depends on `rayon` (threads) and file I/O
- Purpose: a comprehensive collection of secret-detection regex patterns
- Useful as a pattern reference; patterns can be extracted and embedded

### `secretscout`

- Version: 3.1.0
- WASM compatibility: unknown / not confirmed
- Covers cloud provider keys, tokens, private keys
- Again useful as a pattern reference only

### `redact`

- Purpose: type-wrapper (`Secret<T>`) that displays as `[REDACTED]`
- WASM compatible
- Not useful here — operates on typed values, not arbitrary text strings

### `redactable`

- Purpose: derive macros for struct field redaction
- WASM compatible
- Not useful here — operates on struct fields, not arbitrary text

## Pattern sources

The most battle-tested pattern sets for secrets in shell output / command
lines are maintained by:

1. **gitleaks** (`config/gitleaks.toml`) — 130+ rules covering AWS keys,
   GitHub tokens, Slack tokens, SSH private keys, generic high-entropy strings,
   GCP service account keys, JWT tokens, etc.
   https://github.com/gitleaks/gitleaks/blob/master/config/gitleaks.toml

2. **truffleHog** — detector-based (not pure regex, uses entropy scoring too)

3. **Biome** (`no_secrets` lint rule) — embedded regex patterns for common
   API key formats in source code.

## Recommended pattern set for v1

For a v1 implementation embedded in `clank-transcript`, the following
categories cover the most common sensitive values likely to appear in shell
command output or command text:

| Category | Example pattern |
|---|---|
| Generic API key (`key=<value>`) | `(?i)(api[_-]?key\|token\|secret\|password\|passwd\|pwd)\s*[=:]\s*\S+` |
| AWS access key ID | `AKIA[0-9A-Z]{16}` |
| AWS secret access key | `(?i)aws.{0,20}secret.{0,20}[0-9a-zA-Z/+]{40}` |
| GitHub token | `ghp_[a-zA-Z0-9]{36}` / `github_pat_[a-zA-Z0-9_]{82}` |
| Generic JWT | `ey[a-zA-Z0-9_-]+\.ey[a-zA-Z0-9_-]+\.[a-zA-Z0-9_-]+` |
| Private key header | `-----BEGIN .* PRIVATE KEY-----` |
| Bearer token in command | `(?i)bearer\s+[a-zA-Z0-9_\-\.]+` |
| Passwords in command args | `(?i)(--password\|--passwd\|-p)\s+\S+` |
| Email address (PII) | `[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}` |
| IPv4 address | `\b(?:\d{1,3}\.){3}\d{1,3}\b` |
| Credit card (Luhn candidates) | `\b(?:\d[ -]?){13,16}\b` |

These patterns are conservative. High false-positive patterns (IPv4, email)
should be opt-in rather than always-on.

## `regex` crate and `wasm32-wasip2`

The `regex` crate compiles to `wasm32-wasip2` without any feature flags or
conditional compilation. The `regex-syntax` and `aho-corasick` sub-crates
it depends on are also pure Rust with no OS dependencies. `Regex::new()` and
`replace_all()` work identically on native and WASM targets.

The only concern is compile time and binary size: pre-compiling a
`RegexSet` of all patterns once at startup (via `OnceLock`) avoids repeated
compilation and keeps the per-call cost to a single linear scan over the
input string.

## `regex` version and approval

`regex` is already a transitive dependency of `brush-core` (via `brush-parser`)
in the workspace. Adding it as a direct dependency of `clank-transcript`
does not introduce any new code into the dependency tree — it pins an already-
present crate. Approval to add it as a direct dependency is still required per
project conventions.
