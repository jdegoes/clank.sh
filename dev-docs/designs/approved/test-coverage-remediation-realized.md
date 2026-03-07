---
title: "Realized design: Test coverage remediation"
date: 2026-03-07
author: agent
plan: "dev-docs/plans/approved/test-coverage-remediation.md"
---

# Realized design: Test coverage remediation

## What was built

80+ new tests across 15 files covering behaviours that were previously untested or tested
inadequately. Every new test makes a meaningful assertion about a real behavioural contract.

### `clank-http` — `HttpError` and `MockHttpClient`

- Display strings for all four `HttpError` variants locked in (user-visible error messages).
- `From<reqwest::Error>` conversion: timeout maps to `Timeout`, connection refused maps to
  `ConnectionFailed`. Verified with real reqwest errors via a zero-timeout client and an
  unreachable loopback port.
- `MockHttpClient` behaviour: request recording, 503 auto-conversion to
  `NonSuccessResponse`, empty-queue panic.

### `clank-vfs` — `VfsError`, `MockVfs`, `ProcHandler`, `LayeredVfs`

- `VfsError` display strings for all three variants (shown in `cat`, `ls`, `grep`, `stat`
  output).
- `MockVfs::read_dir` empty-vs-NotFound branch (the non-obvious case that distinguishes
  "empty directory" from "path does not exist").
- `ProcHandler` wire format contracts: NUL-separated `cmdline`, `status` line structure,
  NUL-separated `environ` key-value pairs. If these change, downstream tools break silently.
- `LayeredVfs` routing: `/proc/` paths go to the handler, not `RealFs`.

### `clank-shell/src/commands/` — all seven commands

`CatProcess`, `GrepProcess`, `LsProcess`, `StatProcess`, `MkdirProcess`, `RmProcess`,
`TouchProcess`, `EnvProcess` all gained Level 1 unit tests covering: read/output contracts,
flag semantics, missing-file error paths, exit code contracts.

Key contracts locked in:
- `grep -i`: case-insensitive matching
- `grep -n`: line numbers
- `grep -l`: files-only output
- `grep -r`: recursive directory search
- `ls -a`: hidden file visibility
- `ls -l`: long format
- `rm -r`: directory removal
- `rm -f`: suppress errors for missing files
- `env`: secrets masked as `***`

### `clank-shell/src/secrets.rs`

`SecretsRegistry::remove` correctness and `snapshot` consistency. The export/unexport
lifecycle is a security contract.

### `context summarize` (structural fix + tests)

`ContextProcess::with_config` constructor added for test injection. Six tests covering:
empty transcript, success output, timeout exit 3, HTTP error exit 4, parse failure
graceful degradation.

### `clank/tests/processes.rs`

`AskProcess` transcript append contract: the central loop of the application — ask a
question, get a response, record it in the transcript. Tests: response appended on success,
nothing appended on error, stdout routing, stderr routing.

### `clank-shell/tests/authorization.rs`

SudoOnly enforcement: `rm` and `kill` exit 5 without `sudo`; `sudo rm` dispatches (not
denied); Allow commands work without `sudo`.

### `clank-manifest` authorization policies

Table-driven tests asserting `rm` and `kill` are `SudoOnly`, eight write commands are
`Confirm`, and read-only tools are `Allow`. The manifest is the source of truth for what
the authorization system enforces.

### Weak tests replaced

- `test_ask_config_missing_file_returns_error` — was testing `std::fs::read_to_string`,
  not `AskConfig`. Now tests `AskConfig::load` returning `ConfigError::NotFound`.
- `test_model_list_no_config` — now uses isolated config and asserts specific string.
- `test_context_show_empty` — tightened predicate.
- Two `context_summarize_*` tests that acknowledged they couldn't test the actual behaviour
  — replaced by real behavioural tests.
- Duplicate `ask_no_config.yaml` scenario — deleted.

## What was explicitly not added

Tests that would verify standard library behaviour rather than our code: `MockVfs` happy
paths, `NativeHttpClient`, `process_table::kill()`, `ModelProcess::run()` (covered by
scenario tests), `build_system_prompt` exact wording, `wire.rs` serialisation.

All decisions documented in the originating plan.
