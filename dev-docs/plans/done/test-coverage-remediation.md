---
title: "Plan: Test coverage remediation"
date: 2026-03-06
author: agent
issue: "dev-docs/issues/open/test-coverage-remediation.md"
---

# Plan: Test coverage remediation

## Originating Issue

`dev-docs/issues/open/test-coverage-remediation.md` — systematic audit identified code that
violates AGENTS.md mandatory coverage rules and, more importantly, code whose behaviour
could regress silently with real user-visible consequences.

## Guiding principle

Every test in this plan must make a meaningful assertion about a real behavioural contract.
The question for each item is: *what user-visible failure would go undetected without this
test?* Tests that merely verify standard library behaviour, duplicate existing coverage, or
assert that code returns without panicking do not qualify.

---

## What is being fixed and why

### F1 — `HttpError` display strings and `From<reqwest::Error>` conversion

**File:** `crates/clank-http/src/lib.rs`

**Why it matters:** The display strings are what users see when a network call fails
(`"request timed out"`, `"connection failed: …"`, etc.). A silent regression in wording
changes the user-visible error contract. The `From<reqwest::Error>` conversion is the sole
translation point from reqwest failures into our typed `HttpError` enum. If the `is_timeout()`
branch were removed, timeout errors would silently become `ConnectionFailed`, causing wrong
exit codes (4 instead of 3) and wrong messages with no test to catch it.

**Tests to add (Level 1, inline):**

```
test_http_error_display_timeout
test_http_error_display_connection_failed
test_http_error_display_non_success
test_http_error_display_tls
test_http_error_from_reqwest_timeout
test_http_error_from_reqwest_connect_failure
test_http_error_from_reqwest_status_error
test_http_error_from_reqwest_other_falls_through_to_connection_failed
test_mock_http_client_records_request_and_returns_response
test_mock_http_client_non_2xx_via_new_converts_to_error
test_mock_http_client_panics_on_empty_queue
```

The `From<reqwest::Error>` tests construct reqwest errors using
`reqwest::Error`-producing methods (timeout via a zero-timeout client, connect via an
unreachable address). These are fast async tests using `#[tokio::test]` since they touch
reqwest's internal error types, not the network.

### F2 — `VfsError` display strings

**File:** `crates/clank-vfs/src/lib.rs`

**Why it matters:** `VfsError` display strings appear in the output of `cat`, `ls`, `grep`,
and `stat` when a file is not found or permission is denied. If `"not found: {path}"` became
`"no such file"` or lost the path, error output from these commands would silently degrade.

**Tests to add (Level 1, inline):**

```
test_vfs_error_not_found_display_includes_path
test_vfs_error_permission_denied_display_includes_path
test_vfs_error_io_display_includes_path_and_source
```

### F3 — `MockVfs::read_dir` non-obvious branch

**File:** `crates/clank-vfs/src/lib.rs`

**Why it matters:** `read_dir` on a path that has no files in the mock *and* is not itself a
registered key returns `NotFound`. This is the non-obvious branch that distinguishes "empty
directory" from "path does not exist". If this logic were inverted, `ls` on a non-existent
directory would return an empty listing instead of an error. The other `MockVfs` operations
are simple HashMap lookups not worth testing independently.

**Tests to add (Level 1, inline):**

```
test_mock_vfs_read_dir_returns_direct_children_only
test_mock_vfs_read_dir_returns_not_found_for_absent_path
test_mock_vfs_read_dir_returns_empty_for_registered_dir_with_no_children
```

The third test covers the case where a path is registered as a file *and* happens to have
children — confirming that `read_dir` returns the children, not `NotFound`.

### F4 — `ProcHandler` file format contracts

**File:** `crates/clank-vfs/src/proc_handler.rs`

**Why it matters:** The format of `/proc/<pid>/cmdline` (NUL-separated argv), `/proc/<pid>/status`
(line-oriented key-value), and `/proc/<pid>/environ` (NUL-separated KEY=value) are contracts
that shell commands and external tools depend on. If the NUL separator in `cmdline` were
accidentally changed to a space, `xargs` pipelines would silently break. If `status` lost the
`PPid:` field, tools parsing it would fail silently.

**Tests to add (Level 1, inline):**

```
test_proc_handler_cmdline_nul_separated
test_proc_handler_status_format
test_proc_handler_environ_nul_separated_key_value
test_proc_handler_system_prompt_with_source
test_proc_handler_system_prompt_without_source_returns_fallback
test_proc_handler_read_file_unknown_pid_returns_not_found
test_proc_handler_read_file_unknown_subfile_returns_not_found
test_proc_handler_read_dir_proc_root_includes_pids_and_clank
test_proc_handler_read_dir_pid_subdir_lists_three_files
test_proc_handler_stat_proc_is_dir
test_proc_handler_stat_pid_dir_is_dir
test_proc_handler_stat_pid_file_is_file
test_proc_handler_stat_unknown_pid_returns_not_found
```

### F5 — `LayeredVfs` routing contract

**File:** `crates/clank-vfs/src/lib.rs`

**Why it matters:** `LayeredVfs` is the mechanism that makes `/proc/` reads go to `ProcHandler`
instead of the real filesystem. If the routing logic broke, every `/proc/` read would hit `RealFs`
and either return wrong data or fail. One test for the routing contract is sufficient; we don't
need to test every path variant.

**Tests to add (Level 1, inline):**

```
test_layered_vfs_routes_mounted_prefix_to_handler
test_layered_vfs_falls_through_to_real_fs_for_unmounted_path
test_layered_vfs_first_matching_mount_wins
```

### F6 — `CatProcess` behavioural contracts

**File:** `crates/clank-shell/src/commands/cat.rs`

**Why it matters:** `cat` is a fundamental composition tool. The key behavioural contracts:
reading a file via VFS and emitting its bytes; emitting exit 1 with an error message when the
file is not found; reading stdin when no paths are given. These are all user-visible.

**Tests to add (Level 1, inline in `cat.rs`):**

```
test_cat_reads_file_via_vfs
test_cat_missing_file_exits_1_with_error_on_stderr
test_cat_multiple_files_emits_all_contents
test_cat_multiple_files_continues_after_error
test_cat_no_paths_reads_stdin
```

### F7 — `GrepProcess` behavioural contracts

**File:** `crates/clank-shell/src/commands/grep.rs`

**Why it matters:** `grep` is the primary search tool available to the model. The core
contracts are: match found → exit 0; no match → exit 1; missing pattern → exit 2; `-i`
case-insensitive matching; `-n` line numbers; `-l` files-only output; missing file → exit 2
with error on stderr. These are POSIX contracts users depend on.

**Tests to add (Level 1, inline in `grep.rs`):**

```
test_grep_match_found_exits_0
test_grep_no_match_exits_1
test_grep_missing_pattern_exits_2
test_grep_case_insensitive_flag
test_grep_line_numbers_flag
test_grep_files_only_flag
test_grep_missing_file_exits_2_with_error
test_grep_stdin_when_no_paths_given
test_grep_recursive_finds_in_subdirectory
```

### F8 — `LsProcess` behavioural contracts

**File:** `crates/clank-shell/src/commands/ls.rs`

**Why it matters:** `ls` is the primary directory exploration tool. The contracts: directory
listing shows contents; hidden files are filtered by default; `-a` shows hidden files; `-l`
shows long format; missing path emits exit 1 with error. The hidden-file filtering is
particularly important because it affects what the model sees when exploring a project.

**Tests to add (Level 1, inline in `ls.rs`):**

```
test_ls_directory_shows_contents
test_ls_hidden_files_filtered_by_default
test_ls_dash_a_shows_hidden_files
test_ls_long_format
test_ls_single_file_prints_name
test_ls_missing_path_exits_1_with_error
```

### F9 — `StatProcess` behavioural contracts

**File:** `crates/clank-shell/src/commands/stat_cmd.rs`

**Why it matters:** `stat` provides the model with metadata needed to reason about files
before operating on them (is it a directory? how large?). The contracts: displays file type
and size; distinguishes directory from regular file; exit 1 with error for missing path.

**Tests to add (Level 1, inline in `stat_cmd.rs`):**

```
test_stat_file_shows_size_and_type
test_stat_directory_shows_directory_type
test_stat_missing_path_exits_1_with_error
test_stat_missing_operand_exits_1
```

### F10 — `MkdirProcess`, `RmProcess`, `TouchProcess` error path contracts

**Files:** `mkdir.rs`, `rm.rs`, `touch.rs`

**Why it matters:** The happy paths for these commands are incidentally exercised by shell
integration tests. The specific contracts that aren't covered are: missing operand → exit 1
with specific message; `-p` creating nested directories; `-f` suppressing rm errors; `-r`
removing directories recursively. These are the behaviours that differ from a trivial
implementation and that the model will rely on.

**Tests to add (Level 1, inline in each file):**

```
# mkdir.rs
test_mkdir_missing_operand_exits_1
test_mkdir_creates_directory
test_mkdir_p_creates_nested_directories

# rm.rs
test_rm_missing_operand_exits_1
test_rm_removes_file
test_rm_r_removes_directory
test_rm_f_suppresses_error_for_missing_file

# touch.rs
test_touch_missing_operand_exits_1
test_touch_creates_new_file
test_touch_existing_file_exits_0
```

### F11 — `EnvProcess` secrets masking contract

**File:** `crates/clank-shell/src/commands/env_cmd.rs`

**Why it matters:** The secrets masking behaviour is a security contract. If `export --secret`
marks a variable as secret but `env` then prints its value in plaintext, the feature silently
fails. This is the one behaviour in `EnvProcess` that isn't trivially obvious.

**Note:** `ctx.env` is populated from `ProcessContext::env`. In tests this is a plain
`HashMap` we construct directly, so we can inject secrets into it without needing a full
shell. The test needs a real `SecretsRegistry::insert()` call to set up the secret state.

**Tests to add (Level 1, inline in `env_cmd.rs`):**

```
test_env_masks_secret_variables
test_env_prints_non_secret_variables_plaintext
test_env_empty_env_produces_no_output
```

### F12 — `SecretsRegistry::remove()` contract

**File:** `crates/clank-shell/src/secrets.rs`

**Why it matters:** `remove()` is the unexport lifecycle. If it silently failed, a variable
would remain masked as secret indefinitely even after being unexported. This is both a
correctness and a usability contract.

**Tests to add (Level 1, inline in `secrets.rs`):**

```
test_secrets_registry_insert_and_contains
test_secrets_registry_remove_makes_variable_not_secret
test_secrets_registry_snapshot_reflects_current_state
```

### F13 — `context summarize` structural defect fix and tests

**File:** `crates/clank-shell/src/context_process.rs`

**Why it matters:** `context summarize` is untestable in its current form because
`clank_http_config()` reads config from disk instead of using the injected `http` field.
The exit-3 (timeout), exit-4 (HTTP error), and parse-failure paths all carry specific exit
code contracts that are completely untested. The empty-transcript path has a specific output
contract that is also untested.

**Structural fix required:** Change `ContextProcess::summarize` to accept an `AskConfig`
injected at construction time (or passed as a parameter). The `clank_http_config()` helper
stays in place for the production call path but the internal summarize logic uses the
injected config. This is a small refactor: add `config: Option<Arc<AskConfig>>` to
`ContextProcess`, defaulting to `None` (load from disk) in production.

**Tests to add (Level 2, in `crates/clank-shell/tests/context.rs`):**

```
test_context_summarize_empty_transcript_exits_0_with_message
test_context_summarize_success_appends_response_to_stdout
test_context_summarize_timeout_exits_3
test_context_summarize_http_error_exits_4
test_context_summarize_parse_failure_exits_1
test_context_summarize_no_config_exits_1_with_hint
```

Replace the two existing weak tests (`test_context_summarize_calls_model` and
`test_context_summarize_exits_0_or_1`) with these.

### F14 — `AskProcess::run()` AI response → transcript contract

**File:** `crates/clank/src/processes.rs`

**Why it matters:** The central loop of the application: ask a question, get a response,
record it in the transcript so future `ask` calls have context. If the transcript append
were removed, each `ask` call would operate without context from prior ones — the defining
feature of clank would silently break. This test needs to live in `clank/tests/` since
`AskProcess` lives in the binary crate and depends on both `clank-shell` and `clank-ask`.

**Tests to add (Level 2, new file `crates/clank/tests/processes.rs`):**

```
test_ask_process_appends_ai_response_to_transcript
test_ask_process_does_not_append_on_error
test_ask_process_routes_stdout_to_io_handle
test_ask_process_routes_stderr_to_io_handle
```

### F15 — Authorization enforcement (SudoOnly, exit 5)

**File:** `crates/clank-shell/src/shell.rs` (tested via `run_line()`)

**Why it matters:** This is a security contract. `rm` is registered as `SudoOnly`. If the
enforcement check were accidentally removed, the model could delete files without authorization.
The exit code 5 and the specific message are the contract. Currently only the allow path is
tested.

**Tests to add (Level 2, in `crates/clank-shell/tests/context.rs` or new `authorization.rs`):**

```
test_sudo_only_command_without_sudo_prefix_exits_5
test_sudo_only_command_with_sudo_prefix_is_dispatched
test_confirm_policy_command_exits_5_without_sudo
```

Note: `Confirm` commands require interactive user confirmation, which cannot be tested at
Level 2 without a mock for the prompt. The enforcement test — that a `Confirm` command
*without* sudo is still blocked — can be tested if the shell is configured to deny by default
in non-interactive mode. This is worth implementing; the current state where `Confirm`
enforcement is completely untested is a gap.

### F16 — `dispatch_builtin` exit code truncation

**File:** `crates/clank-shell/src/builtins.rs` (tested via `run_line()`)

**Why it matters:** `result.exit_code as u8` silently wraps exit codes above 255 to 0,
turning an error into a success. An `ask` call returning exit 256 would become 0. This is
a concrete data loss bug.

**Fix required:** Change `as u8` to `as i32` (or use the raw `i32` value directly, which
is what `ProcessResult::exit_code` already is).

**Test to add (Level 2):**

```
test_dispatch_builtin_exit_code_above_255_not_truncated
```

This test registers a mock `Process` that returns `ProcessResult::failure(256)`, calls it
via `run_line()`, and asserts the shell returns 256 (not 0).

### F17 — `clank-manifest` authorization policy assertions

**File:** `crates/clank-manifest/src/lib.rs`

**Why it matters:** The manifest registry is the source of truth for what the authorization
system enforces. If `rm` were accidentally registered as `Allow` instead of `SudoOnly`, the
shell's authorization check would stop protecting it and the test suite would have no signal.

**Tests to add (Level 1, inline in `lib.rs`):**

```
test_manifest_sudo_only_commands
test_manifest_confirm_commands
test_manifest_allow_commands_are_not_elevated
```

These are table-driven. `test_manifest_sudo_only_commands` iterates `["rm", "kill"]` and
asserts `authorization_policy == SudoOnly`. `test_manifest_confirm_commands` iterates
`["curl", "wget", "cp", "mv", "mkdir", "touch", "tee", "patch"]` and asserts `Confirm`.

### F18 — Existing weak tests: fix or replace

**Why it matters:** Tests that would pass on incorrect behaviour give false confidence.

Changes:

1. **Delete** `crates/clank/tests/scenarios/ask/ask_no_config.yaml` — byte-for-byte
   duplicate of `ask_stub.yaml`. No coverage value.

2. **Fix** `test_model_list_no_config` in `model_process.rs`: the assertion
   `out.contains("No providers configured") || out.contains("Default model")` accepts two
   contradictory outcomes. With an isolated empty config, the output is always
   `"No providers configured.\n..."`. Assert on the specific string.

3. **Fix** `test_context_show_empty` in `crates/clank/tests/ask.rs`: the predicate
   `contains("empty").or(contains("context"))` passes on any output containing `"context"`.
   Tighten to `contains("empty")` alone, which is the actual contract.

4. **Remove** `test_context_summarize_calls_model` and `test_context_summarize_exits_0_or_1`
   from `crates/clank-shell/tests/context.rs` once F13 is implemented. They acknowledge in
   comments that they cannot test actual behaviour.

---

## What is explicitly NOT included and why

- **`NativeHttpClient` unit tests** — requires a live network or a local mock server; no
  unit-testable behaviour beyond what the provider tests already cover indirectly.
- **`process_table::kill()` and `set_abort_handle()`** — task cancellation is not yet a
  user-visible feature. Testing `AbortHandle::abort()` would test the tokio runtime, not our code.
- **`ModelProcess::run()` dedicated test** — four lines routing `run_model()` output; already
  covered by scenario tests.
- **`AskFlags::parse` direct unit tests** — already thoroughly exercised through `run_ask`
  tests; adding direct tests would duplicate that coverage without catching new failures.
- **`build_system_prompt` exact wording** — the prompt is a design decision, not a behavioural
  contract visible to users. Locking in exact wording would make every prompt improvement a
  test change.
- **`MockVfs` happy-path tests** — `read_file` and `stat` are HashMap lookups; testing them
  verifies the standard library.
- **`RealFs` unit tests** — already exercised by every shell integration test that touches the
  real filesystem.
- **`LayeredVfs` with overlapping prefixes** — no current feature depends on this behaviour.
- **`MockHttpClient::recorded_requests()` drain semantics** — testing VecDeque drain is
  testing the standard library.
- **`ProviderError::Other` display** — used only for internal serialization failures; not a
  user-facing contract distinguishable from other errors.
- **`wire.rs` serialization** — already covered by every provider round-trip test.

---

## Changed files summary

| File | Change type | Tasks |
|---|---|---|
| `crates/clank-http/src/lib.rs` | Add unit tests | F1 |
| `crates/clank-vfs/src/lib.rs` | Add unit tests | F2, F3, F5 |
| `crates/clank-vfs/src/proc_handler.rs` | Add unit tests | F4 |
| `crates/clank-shell/src/commands/cat.rs` | Add unit tests | F6 |
| `crates/clank-shell/src/commands/grep.rs` | Add unit tests | F7 |
| `crates/clank-shell/src/commands/ls.rs` | Add unit tests | F8 |
| `crates/clank-shell/src/commands/stat_cmd.rs` | Add unit tests | F9 |
| `crates/clank-shell/src/commands/mkdir.rs` | Add unit tests | F10 |
| `crates/clank-shell/src/commands/rm.rs` | Add unit tests | F10 |
| `crates/clank-shell/src/commands/touch.rs` | Add unit tests | F10 |
| `crates/clank-shell/src/commands/env_cmd.rs` | Add unit tests | F11 |
| `crates/clank-shell/src/secrets.rs` | Add unit tests | F12 |
| `crates/clank-shell/src/context_process.rs` | Structural fix + tests | F13 |
| `crates/clank-shell/src/builtins.rs` | Fix `as u8` bug + test | F16 |
| `crates/clank-manifest/src/lib.rs` | Add unit tests | F17 |
| `crates/clank/tests/processes.rs` | New Level 2 test file | F14 |
| `crates/clank-shell/tests/authorization.rs` | New Level 2 test file | F15 |
| `crates/clank-shell/tests/context.rs` | Replace weak tests | F13 |
| `crates/clank/tests/scenarios/ask/ask_no_config.yaml` | Delete | F18 |
| `crates/clank-ask/src/model_process.rs` | Fix weak assertion | F18 |
| `crates/clank/tests/ask.rs` | Tighten predicate | F18 |

---

## Acceptance tests

The plan is complete when:

1. `cargo test --workspace` passes with zero failures.
2. `cargo clippy --all-targets -- -D warnings` passes.
3. `cargo fmt --check` passes.
4. Every item in the tasks list is checked.
5. The `dispatch_builtin` exit code truncation bug (`as u8`) is fixed in source, not just
   detected by a test.
6. `context summarize` can be tested with a `MockHttpClient` — the structural defect is
   resolved.

---

## Tasks

- [ ] **F1** Add `HttpError` display and `From<reqwest::Error>` unit tests to `clank-http/src/lib.rs`; include `MockHttpClient` behaviour tests
- [ ] **F2** Add `VfsError` display unit tests to `clank-vfs/src/lib.rs`
- [ ] **F3** Add `MockVfs::read_dir` non-obvious branch tests to `clank-vfs/src/lib.rs`
- [ ] **F4** Add `ProcHandler` file format contract tests to `clank-vfs/src/proc_handler.rs`
- [ ] **F5** Add `LayeredVfs` routing contract tests to `clank-vfs/src/lib.rs`
- [ ] **F6** Add `CatProcess` behavioural contract tests to `commands/cat.rs`
- [ ] **F7** Add `GrepProcess` behavioural contract tests to `commands/grep.rs`
- [ ] **F8** Add `LsProcess` behavioural contract tests to `commands/ls.rs`
- [ ] **F9** Add `StatProcess` behavioural contract tests to `commands/stat_cmd.rs`
- [ ] **F10** Add `MkdirProcess`, `RmProcess`, `TouchProcess` error path and flag tests
- [ ] **F11** Add `EnvProcess` secrets masking contract tests to `commands/env_cmd.rs`
- [ ] **F12** Add `SecretsRegistry::remove()` and `snapshot()` tests to `secrets.rs`
- [ ] **F13** Fix `context summarize` structural defect (inject config); replace weak tests with real behavioural tests
- [ ] **F14** Add `AskProcess::run()` transcript append and routing tests in new `crates/clank/tests/processes.rs`
- [ ] **F15** Add authorization enforcement tests in new `crates/clank-shell/tests/authorization.rs`
- [ ] **F16** Fix `dispatch_builtin` exit code truncation (`as u8` → `as i32`); add regression test
- [ ] **F17** Add table-driven authorization policy assertions to `clank-manifest/src/lib.rs`
- [ ] **F18** Delete duplicate `ask_no_config.yaml`; fix `test_model_list_no_config` assertion; tighten `test_context_show_empty` predicate; remove two acknowledged-weak summarize tests (after F13 replaces them)
- [ ] **QG** `cargo test --workspace`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check` all pass
