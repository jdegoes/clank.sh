---
title: "Realized design: Scenario test harness"
date: 2026-03-07
author: agent
plan: "dev-docs/plans/approved/scenario-test-harness.md"
---

# Realized design: Scenario test harness

## What was built

A custom test harness replacing `trycmd` with a single-file YAML fixture format and a
~400-line Rust runner.

### Fixture format

**Location:** `crates/clank/tests/scenarios/<feature>/<case>.yaml`

Each `.yaml` file captures the complete test case:

```yaml
desc: "human-readable description"
env:
  CLANK_CONFIG: "{config}"   # {config} → isolated temp path (default)
config:                       # initial config TOML as structured data (optional)
  providers:
    anthropic:
      api_key: "sk-test"
files:                        # pre-populated sandbox files (optional)
  "scripts/foo.sh": "#!/bin/sh\necho hi\n"
stdin: |                      # commands sent to stdin
  echo hello
stdout: "$ hello\n$ \n"       # expected stdout (exact)
stderr: ""                    # expected stderr (exact)
config_after:                 # post-session config assertions (subset match)
  providers:
    ollama:
      base_url: "http://localhost:11434"
files_after:                  # post-session file content assertions
  "out.txt": "hello\n"
```

### Runner: `scenario.rs`

**File:** `crates/clank/tests/scenario.rs`

Key properties:

- **Always-isolated config.** Every test gets a `tempfile::TempDir`. `CLANK_CONFIG` is
  set to `<tempdir>/ask.toml` unconditionally unless the fixture explicitly overrides it.
  No test can accidentally read `~/.config/ask/ask.toml`.
- **`config:` pre-population.** If present, serialised to TOML and written to the
  isolated config path before the process is spawned.
- **`config_after:` subset assertions.** `assert_toml_subset` checks only the fields
  listed in the fixture; other fields are ignored. A `model add` test does not break if
  the schema later gains a new field.
- **`{config}` and `{cwd}` token expansion.** Available in `env` values and expected
  output strings for path-dependent assertions.
- **`CLANK_UPDATE=1` regeneration.** Runs the binary and updates `stdout:`/`stderr:` in
  the fixture YAML in place.
- **Name filtering.** `cargo test --test scenario -- scenario_tests <substring>` runs
  only matching fixtures.

### Migration from trycmd

All 14 previously-passing trycmd fixtures migrated to YAML. Four fixtures that previously
had no output assertions (`ask_no_config`, `context_clear`, `context_show_empty`,
`model_list_no_config`) now assert real stdout and stderr. Three model fixtures now have
`config_after:` assertions.

`trycmd`, `golden.rs`, and `tests/fixtures/` removed. AGENTS.md updated.

## Decisions made during implementation

**YAML over TOML.** YAML block scalars (`|`) are unambiguously cleaner for multi-line
terminal output than TOML triple-quoted strings. `serde_yaml` is the only new dependency.

**Custom runner over trycmd extension.** Full control over config isolation semantics,
`config_after` assertions, and fixture format. ~400 lines; readable in one sitting.

**`config:` as structured TOML, not a file path.** The fixture author writes the
configuration intent, not file bytes. This makes fixtures easier to read and immune to
serialisation format changes.

**Subset semantics for `config_after:`.** Only asserted fields are checked. This makes
tests resilient to schema additions without requiring fixture updates.

## Test coverage

14 scenario fixtures covering: shell basics (echo, boolean operators, pipes, export,
multiline), ask/context commands (stub, no-config, context clear/show/trim, model list),
and model commands (add ollama, add openai-compat error, list with local providers).

All scenarios pass. `cargo test --test scenario` confirms.
