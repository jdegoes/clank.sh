---
title: "Transcript redaction — heuristic pattern scrubbing and manifest redaction-rules"
date: 2026-03-07
author: agent
issue: "dev-docs/issues/open/transcript-redaction.md"
research:
  - "dev-docs/research/rust-sensitive-data-redaction.md"
designs:
  - "dev-docs/designs/proposed/transcript-and-context-builtin.md"
---

# Transcript redaction — heuristic pattern scrubbing and manifest redaction-rules

## Originating Issue

Transcript does not redact sensitive values — secrets, tokens, PII will be
sent to the model. See `dev-docs/issues/open/transcript-redaction.md`.

## Design Clarification

The issue described two mechanisms. The developer confirmed the correct
architecture from the README:

> Redaction rules apply at all times. Anything governed by a `redaction-rules`
> entry in a **command manifest** never enters the transcript.

`redaction-rules` is a **manifest field**, not a runtime variable-marking
mechanism. It is declared per-command at install time (or for builtins, in
Rust), listing which argument names have values that must never enter the
transcript. The `export --secret` syntax mentioned in the issue is a separate
concern deferred to a later plan.

This plan implements:
1. **Heuristic regex redaction** — applied to all `Output` entries and
   `Command` entries before storage.
2. **Manifest `redaction-rules`** — the `CommandManifest` type gains
   `redaction_rules: Vec<String>`; the recording call site scrubs matching
   argument values from `Command` entries.

## Research Consulted

`dev-docs/research/rust-sensitive-data-redaction.md` — confirms `regex` is
the correct approach; WASM-compatible; already a transitive dependency.

## Developer Feedback

`redaction-rules` is a top-level command manifest field, similar in shape to
`execution-scope`. It is always applied — there is no opt-in at runtime.
Patterns are set at the manifest level, not at runtime via `export --secret`.

## Design

### New dependency: `regex`

`clank-transcript/Cargo.toml` gains:

```toml
regex = { version = "1", default-features = false, features = ["std"] }
```

`default-features = false` avoids the `perf-*` features (which pull in
`aho-corasick` extras) for binary size. The `std` feature is required.
`regex` is already present in the workspace as a transitive dependency of
`brush-parser`; adding it directly to `clank-transcript` pins the version
explicitly.

### `Redactor` in `clank-transcript`

A new `Redactor` struct in `clank-transcript/src/lib.rs` (or a new
`clank-transcript/src/redactor.rs` module) owns the compiled regex patterns
and exposes a single method:

```rust
pub struct Redactor {
    patterns: Vec<(Regex, &'static str)>,  // (pattern, label for replacement)
}

impl Redactor {
    /// Build with the default pattern set (all always-on patterns).
    pub fn default() -> Self { ... }

    /// Build with no patterns — used in tests to avoid false positives.
    pub fn none() -> Self { Self { patterns: vec![] } }

    /// Scrub `text`, replacing any matched sensitive value with [REDACTED].
    /// Returns the scrubbed string. If no patterns match, returns the
    /// original string unchanged (no allocation).
    pub fn scrub(&self, text: &str) -> String { ... }

    /// Scrub a list of exact literal values from `text` (for manifest
    /// redaction-rules argument values). Each matched literal is replaced
    /// with [REDACTED].
    pub fn scrub_literals(&self, text: &str, literals: &[&str]) -> String { ... }
}
```

The `scrub` method iterates patterns and calls `regex.replace_all(text,
"[REDACTED]")` for each. Because patterns are pre-compiled, each call is a
linear scan. The `scrub_literals` method does exact substring replacement for
declared-secret argument values found in command text.

### Default pattern set (always-on)

These patterns are applied to every entry:

| ID | What it matches | Pattern |
|---|---|---|
| `config-secret` | Any identifier (env var, TOML key, YAML key) whose name contains a sensitive keyword, in assignment or display form, with optional quoting | `(?i)[a-z0-9_]*(?:token\|secret\|password\|passwd\|pwd\|api[_-]?key\|credential\|auth[_-]?key)[a-z0-9_]*\s*[=:]\s*["']?[^\s"',;\]}\n][^"',;\]}\n]*["']?` |
| `generic-secret-flag` | `--key`, `--token`, `--password`, `--secret`, `--passwd` CLI flags followed by a value | `(?i)(--(?:api[_-]?key\|token\|secret\|password\|passwd\|pwd))[=\s]+\S+` |
| `aws-access-key` | AWS access key ID | `AKIA[0-9A-Z]{16}` |
| `aws-secret-key` | AWS secret access key (heuristic) | `(?i)aws.{0,20}secret.{0,20}[0-9a-zA-Z/+]{40}` |
| `github-token` | GitHub personal access token | `ghp_[a-zA-Z0-9]{36}` |
| `github-pat` | GitHub fine-grained PAT | `github_pat_[a-zA-Z0-9_]{82}` |
| `jwt` | JSON Web Token (three base64url segments) | `ey[a-zA-Z0-9_-]{10,}\.ey[a-zA-Z0-9_-]{10,}\.[a-zA-Z0-9_-]+` |
| `pem-private-key` | PEM private key header line | `-----BEGIN [A-Z ]* PRIVATE KEY-----` |
| `bearer-token` | HTTP Authorization Bearer token | `(?i)bearer\s+[a-zA-Z0-9_\-\.]{16,}` |

The `config-secret` pattern is the centrepiece of the default set. It is
fully case-insensitive (`(?i)`) and covers all common config file formats:

**Shell / `.env`:**
- `DB_PASSWORD=hunter2` → `DB_PASSWORD=[REDACTED]`
- `STRIPE_API_TOKEN=sk_live_abc123` → `STRIPE_API_TOKEN=[REDACTED]`
- `export GITHUB_TOKEN=ghp_abc` → `export GITHUB_TOKEN=[REDACTED]`

**TOML:**
- `password = "hunter2"` → `password = [REDACTED]`
- `api_key = "sk_live_abc"` → `api_key = [REDACTED]`
- `secret_token = "xyz789"` → `secret_token = [REDACTED]`

**YAML:**
- `password: hunter2` → `password: [REDACTED]`
- `auth_key: "abc123"` → `auth_key: [REDACTED]`
- `db_credentials: secretvalue` → `db_credentials: [REDACTED]`

The value portion of the pattern accepts optionally-quoted values and matches
to the end of the value (stopping at whitespace, quotes, commas, semicolons,
brackets, or newlines). This handles both bare values and quoted values without
consuming surrounding structure. The replacement preserves the key name and
separator so the transcript remains readable.

Empty assignments (`password=`, `api_key:`) are not redacted — there is no
value to protect.

IPv4 addresses, email addresses, and credit card numbers are explicitly
excluded from the default set due to high false-positive rates; they can be
added via a future configuration mechanism.

### `Transcript` integration

`Transcript::new` gains an optional `Redactor` parameter:

```rust
pub fn new(max_entries: usize) -> Self  // uses Redactor::default()
pub fn with_redactor(max_entries: usize, redactor: Redactor) -> Self
```

`push` applies `redactor.scrub(entry.kind.text())` before storing. The
`Redactor` is owned by `Transcript` — no shared reference needed.

Because the process-global is initialized via `OnceLock`, the default
`Redactor` is constructed once. The global always uses `Redactor::default()`.
Tests that need a no-op redactor construct a local `Transcript` directly
rather than going through the global.

### `redaction_rules` on `CommandManifest`

`CommandManifest` in `clank-builtins/src/lib.rs` gains:

```rust
pub struct CommandManifest {
    pub name: &'static str,
    pub scope: ExecutionScope,
    pub redaction_rules: &'static [&'static str],  // argument names to redact
}
```

`redaction_rules` defaults to `&[]` (no redaction) for all existing entries.
The `MANIFEST_REGISTRY` entries are updated to include the field.

#### Why `echo`, `cat`, `grep` do not use `redaction_rules`

`redaction_rules` declares **named flag arguments** whose values must be
scrubbed from the `Command` entry text. It is not applicable to commands that
receive sensitive data as positional arguments or via stdin — `echo`,
`cat`, and `grep` fall into this category.

- `echo $SECRET` — the expanded value appears as a positional argument.
  There is no flag name to declare. The heuristic regex redactor catches it
  if the value matches a known pattern; otherwise it is outside the scope
  of automatic redaction per the README.
- `cat /etc/secrets` — content flows through captured stdout. The heuristic
  redactor applied to the `Output` entry is the right protection here.
- `grep pattern file` — same as `cat`; positional arguments, no flag names.
- `eval "$cmd $arg"` — the `Command` entry records the literal text
  `eval "$cmd $arg"` before shell expansion; the unexpanded variable
  references are stored, not the secret values. The `Output` entry from
  eval's result is heuristically scrubbed.

`redaction_rules` is meaningful for commands that accept secrets via
**named flags**: e.g. a future `model add --key $API_KEY` would declare
`--key` in its rules, so the entry is stored as `model add --key [REDACTED]`
regardless of whether the key value matches any regex pattern.

For v1, no current builtin declares `redaction_rules` values — the field is
infrastructure for future commands. The `export` builtin is a candidate once
`export --secret KEY=value` is implemented in a later plan.

### Recording call site integration in `clank-core`

`run_statement` in `clank-core/src/lib.rs` is updated to:

1. Look up the manifest entry for `cmd` (by checking the first word against
   `clank_builtins::scope_of` and a new `clank_builtins::redaction_rules_of`
   function).
2. If `redaction_rules` is non-empty, extract the argument values for those
   names from the command text and call `Redactor::scrub_literals` before
   recording the `Command` entry.
3. The `Output` entry text is always passed through the global
   `Transcript`'s embedded redactor automatically via `push`.

The heuristic redaction (step 3) requires no changes to `run_statement` —
it is handled inside `Transcript::push`.

### `redaction_rules_of` in `clank-builtins`

```rust
pub fn redaction_rules_of(name: &str) -> &'static [&'static str] {
    MANIFEST_REGISTRY
        .iter()
        .find(|m| m.name == name)
        .map(|m| m.redaction_rules)
        .unwrap_or(&[])
}
```

### Test strategy

**Unit tests in `clank-transcript`:**
- `scrub` replaces each default pattern category with `[REDACTED]`
- `scrub` does not modify text with no sensitive content
- `scrub_literals` replaces exact literal values
- `scrub_literals` with empty literals returns input unchanged
- `Redactor::none()` returns input unchanged for any input

**Integration tests in `clank-core/tests/transcript.rs`:**
- A command containing a recognisable pattern (e.g. `echo AKIA1234567890ABCDEF`)
  stores a `Command` entry with the AWS key replaced by `[REDACTED]`
- An output entry containing a GitHub token is stored with `[REDACTED]`
- Normal commands without sensitive content are stored verbatim

**Acceptance tests:**
- `echo AKIA1234567890ABCDEF; context show` → stdout contains
  `command: echo [REDACTED]` and `output: [REDACTED]`
- `echo hello; context show` → not redacted (normal content unchanged)

### Note on test isolation

The integration tests in `clank-core/tests/transcript.rs` use the
process-global transcript which has `Redactor::default()`. Tests that
seed the transcript with patterns that happen to match the default redactor
(e.g. tests using `echo AKIA...`) must account for redaction in their
`entries()` assertions. Existing tests that use `echo a`, `echo hello`,
`echo first`, etc. are unaffected — none match any default pattern.

## Tasks

- [ ] Add `regex` as a direct dependency of `clank-transcript` with
      `default-features = false, features = ["std"]`
- [ ] Implement `Redactor` struct in `clank-transcript` with `default()`,
      `none()`, `scrub()`, and `scrub_literals()` methods
- [ ] Add unit tests for `Redactor`: each pattern category (env var uppercase,
      TOML lowercase, YAML lowercase, AWS key, GitHub token, JWT, PEM header,
      Bearer token, generic flag), case-insensitivity of `config-secret`,
      non-sensitive content passes through unchanged, empty assignment not
      redacted, `scrub_literals`, `Redactor::none()`
- [ ] Update `Transcript::new` to embed a `Redactor::default()` and apply
      `scrub` in `push`; add `Transcript::with_redactor` constructor
- [ ] Update `clank-transcript` unit tests that assert on entry text to account
      for redaction (or construct local `Transcript::with_redactor(n, Redactor::none())`
      where needed)
- [ ] Add `redaction_rules: &'static [&'static str]` field to `CommandManifest`
      in `clank-builtins`; update all `MANIFEST_REGISTRY` entries with `&[]`
- [ ] Add `redaction_rules_of(name: &str)` public function to `clank-builtins`
- [ ] Update `run_statement` in `clank-core` to call `scrub_literals` on
      command text using the manifest's `redaction_rules` before recording
      the `Command` entry
- [ ] Add integration tests in `clank-core/tests/transcript.rs` for:
      AWS key in command, GitHub token in output, normal content unchanged
- [ ] Add acceptance test cases in a new
      `clank-acceptance/cases/redaction/patterns.yaml` covering:
      known pattern in command is redacted in show output,
      known pattern in echo output is redacted,
      non-sensitive content is not redacted
- [ ] Run full test suite; verify no regressions

## Acceptance Tests

New file: `clank-acceptance/cases/redaction/patterns.yaml`

- `echo AKIA1234567890ABCDEF; context show` → redacted (AWS access key)
- `echo DB_PASSWORD=hunter2; context show` → `DB_PASSWORD=[REDACTED]` (env var)
- `echo MY_API_TOKEN=sk_live_abc123; context show` → `MY_API_TOKEN=[REDACTED]`
- `printf 'password = "hunter2"'; context show` → `password = [REDACTED]` (TOML)
- `printf 'api_key: sk_live_abc'; context show` → `api_key: [REDACTED]` (YAML)
- `echo ghp_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa; context show` → redacted (GitHub token)
- `echo 'bearer abc123def456ghi789jkl012mno345'; context show` → redacted (Bearer)
- `echo hello; context show` → `hello` unchanged (no false positive)
- `echo MYAPP=hunter2; context show` → `MYAPP=hunter2` unchanged (name not sensitive)

## Out of Scope

- `export --secret KEY=value` runtime variable tracking — separate issue.
- IPv4, email, credit card pattern categories — deferred; high false-positive
  risk requires user-configurable opt-in.
- Redaction of entries already in the transcript — redaction is write-time only.
- `--secret` flag on `prompt-user` — separate mechanism.
- Golem oplog redaction — separate issue.
