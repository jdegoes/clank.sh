---
title: "Code quality: correctness bugs, idiom violations, and stale documentation"
date: 2026-03-06
author: agent
---

# Code quality: correctness bugs, idiom violations, and stale documentation

## Problem

A systematic audit of recent development work identified a set of issues across
correctness, Rust idioms, test quality, and documentation — all cases where an easier
path was taken at the time but produces a lower-quality result than the correct solution.

## Critical correctness bugs

**`env` command silently produces no output** (`builtins.rs:311`): `ProcessContext.env`
is always passed as `HashMap::new()` from `dispatch_builtin`. `EnvProcess` reads from
`ctx.env` to produce its output. The result is that `env` run from the clank shell always
shows an empty environment. Confirmed in testing.

**`SUDO_STATE` is process-wide, not per-shell** (`shell.rs:15`): `SUDO_STATE` is a
single `AtomicBool` shared across all `ClankShell` instances in the same process. A test
that calls `sudo rm` sets the flag. If a concurrently running test's `SudoOnly` command
dispatches before the flag is cleared, it bypasses authorization. Additionally, if
`run_line` returns early (e.g. on a parse error) between `SUDO_STATE.store(true)` and
the clear in `dispatch_builtin`, the flag is left set permanently. This is an authorization
bypass in both test and production contexts.

**Second `u8` exit code truncation in `shell.rs:302`**: `std::process::exit(u8::from(r.exit_code) as i32)` on the `ExitShell` path truncates exit codes above 255 to 0, turning an error exit into apparent success. The same bug was fixed in `dispatch_builtin` but this instance was missed.

## Test quality violations

**`Runtime::new().unwrap().block_on()` in 7 `#[test]` functions**: AGENTS.md requires
`#[tokio::test]` for any test that awaits. The command tests in `grep.rs`, `ls.rs`,
`mkdir.rs`, `rm.rs`, `touch.rs`, `stat_cmd.rs`, and `env_cmd.rs` all use manual runtime
creation instead.

**`test_ask_config_missing_file_returns_error` tests stdlib, not `AskConfig`**
(`config.rs:197–202`): This test calls `std::fs::read_to_string` on a nonexistent path
and asserts it returns an error — a property of the standard library. `AskConfig::load`
is never called. The test produces zero coverage of production code.

**`test_context_summarize_parse_failure` asserts `exit_code == 0 || exit_code == 1`**
(`context.rs:296–303`): The actual behaviour is deterministic (unexpected JSON shape with
no `content` array → exit 0 with empty stdout). The loose `|| == 1` assertion makes the
test compatible with almost any outcome.

**`env_cmd` test teardown happens after assertions** (`env_cmd.rs:55–70`):
`SecretsRegistry::remove("MY_SECRET")` is called after the assertion. If the assertion
panics, the secret remains registered, polluting all subsequent tests in the same process.

**System tests missing required assertions** (`ask.rs`): AGENTS.md requires asserting both
stdout and stderr. `test_ask_no_config_exits_with_message`, `test_ask_bad_args_exits_stderr`,
and `test_model_no_subcommand` assert only stderr. `test_context_show_empty` and
`test_context_clear_succeeds` assert neither.

## Rust idiom violations

**`grep -rn` combined flags do not work** (`grep.rs`): The `-i`, `-n`, `-l`, `-r` flags
are parsed as exact string matches (`a == "-n"`). Combined short flags like `grep -rn`
or `grep -ni` are not recognized. Any user who types combined flags gets no flags applied.

**`pattern.to_lowercase()` recomputed on every line in grep** (`grep.rs:113`): The
`-i` case-insensitive path lowercases the pattern once per line rather than once per
invocation. For a file with thousands of lines this is thousands of redundant allocations.

**`grep_recursive` takes `&Arc<dyn Vfs>` instead of `&dyn Vfs`** (`grep.rs:142`):
Passing `&Arc<T>` instead of `&T` is an anti-pattern; the function has no need to clone
the Arc.

**`let mut ctx = ctx` in two `Process::run` impls** (`processes.rs:25, 80`): The
idiomatic form is `mut ctx: ProcessContext` in the parameter declaration.

**Default model string duplicated in four places**: `"anthropic/claude-sonnet-4-5"` is
a literal in `config.rs`, `model_process.rs`, and `context_process.rs`. A named constant
eliminates the risk of partial updates.

**`ProcHandler::find_proc` clones the entire process list** (`proc_handler.rs:49–55`):
`procs()` clones the full `Vec<ProcessSnapshot>` (including all environment variables for
all processes) on every VFS call. The correct approach holds the read lock for the search
duration and clones only the found entry.

**`resolve_model` clones `Option<String>` unnecessarily** (`config.rs:129`): Can be
written with `or(self.default_model.as_deref())` to avoid the allocation.

**`recorded_requests()` on `MockHttpClient` drains destructively** (`clank-http/src/lib.rs:148`):
A method named `recorded_requests()` should be non-destructive. The destructive semantics
should be signalled by the name `take_recorded_requests()`.

**URL construction with `format!` allows double-slash** (4 provider files): If `base_url`
has a trailing slash, `format!("{}/api/chat", base_url)` produces a double-slash URL.
Trimming at construction time is the correct fix.

**`init_global_registry()` has no idempotency guard** (`clank-manifest/src/lib.rs:177`):
Called from every `ClankShell::new()`, it re-registers every default command on each call.
A `OnceLock` or equivalent ensures single initialization.

## Stale documentation

**`AGENTS.md` lines 282–346 describe `trycmd`/`golden.rs` infrastructure that no longer exists**:
The Build section documents `TRYCMD=overwrite cargo test --test golden` and references
`crates/clank/tests/golden.rs` and `tests/fixtures/`. These were replaced by the YAML
scenario harness. Any agent following this documentation will be confused and fail.

**`AGENTS.md` line 55 calls `wasm32-wasip2` "primary"** but line 10 says it is explicitly
deferred. The contradiction within the same document is confusing.

## Out of scope (filed for later)

- `std::sync::Mutex` in async code (safe as written; no await crossing)
- Manual flag parsing broadly (large scope; `grep -rn` fix is in scope)
- Stringly-typed error returns (`Result<_, String>`) across ask/shell
- `context_process` config re-implementation and provider coupling
- `clank/src/lib.rs` as architectural workaround
- Global dispatch table memory leak on shell drop
- Blocking stdin in `run_interactive`
- `Vfs` trait missing write operations
