---
title: "Realized design: Code quality remediation"
date: 2026-03-07
author: agent
plan: "dev-docs/plans/approved/code-quality-remediation.md"
---

# Realized design: Code quality remediation

## What was built

27 targeted fixes across correctness bugs, Rust idioms, test quality, documentation, and
architecture. All items represent cases where a convenient path was taken but produced a
meaningfully lower-quality result than the correct solution.

---

## Correctness bugs fixed

### `env` command was broken in production (Q1)

`dispatch_builtin` was passing `env: HashMap::new()` to every `ProcessContext`. `EnvProcess`
reads from `ctx.env` — so `env` always printed nothing.

**Fix:** `dispatch_builtin` now calls `current_env_snapshot()` which reads `std::env::vars()`
and masks any secret variable values as `***`. Confirmed: `env` now outputs 40+ variables.

### `SUDO_STATE` was process-wide (Q2)

A single `AtomicBool` was shared across all `ClankShell` instances. Concurrent tests could
bleed sudo authorization across shell instances. A parse error between `SUDO_STATE.store(true)`
and the dispatch clear would leave the flag permanently set.

**Fix:** Replaced with `static SUDO_STATE: LazyLock<RwLock<HashMap<u64, bool>>>` keyed by
`shell_id`. Flag is cleared unconditionally in `run_line` after `run_string` returns, not
inside `dispatch_builtin`. Two isolation tests verify the per-shell and post-command-clear
contracts.

### `ExitShell` exit code truncation (Q3)

`std::process::exit(u8::from(r.exit_code) as i32)` in `shell.rs` was a second instance of
the truncation bug previously fixed in `dispatch_builtin`. Exit code 256 would become 0.

**Fix:** Extracted `exit_code_to_i32()` helper used consistently in both call sites.

---

## Rust idiom fixes

### Combined short flags (Q4)

All seven command implementations (`grep`, `ls`, `rm`, `mkdir`, `touch`, `stat`, `cat`)
parsed flags as exact string matches. `grep -rn` silently applied no flags.

**Fix:** `has_flag(argv, short_char, long_form)` helper in each command. Checks both exact
matches and combined short-flag strings (`-rn` contains `r`, `n`). New tests for `-rn`,
`-la`, `-rf`.

### `grep` pattern lowercased per line (Q5)

`pattern.to_lowercase()` was called once per line for case-insensitive matching. For a
10,000-line file this was 10,000 redundant allocations.

**Fix:** `pattern_lower = ignore_case.then(|| pattern.to_lowercase())` computed once before
the loop.

### `grep_recursive` took `&Arc<dyn Vfs>` (Q6)

Changed to `&dyn Vfs`. No cloning needed; the function only calls methods.

### `DEFAULT_MODEL` constant (Q8)

`"anthropic/claude-sonnet-4-5"` appeared as a literal in four places. Defined once in
`clank-ask/src/config.rs` as `pub const DEFAULT_MODEL: &str`.

### `ProcHandler::find_proc` cloned the full process list (Q9)

Every VFS call (e.g. `cat /proc/1/cmdline`) cloned the entire `Vec<ProcessSnapshot>`
including all environment variables for all processes.

**Fix:** `find_proc` now holds the read lock for the duration of the search and clones only
the found entry. The `procs()` method was removed.

### Provider URL trailing slashes trimmed at construction (Q12)

All four provider `new()` constructors call `trim_end_matches('/')` on `base_url`. A trailing
slash would produce double-slash URLs (`http://localhost:11434//api/chat`).

### `init_global_registry` idempotency (Q13)

Previously re-registered all default commands on every `ClankShell::new()`. Guarded by
`OnceLock` — the first call populates; subsequent calls are no-ops.

---

## Architectural improvements

### `Vfs` trait gained write operations (Q27)

`write_file`, `create_dir`, `create_dir_all`, `remove_file`, `remove_dir_all` added to
the `Vfs` trait. `MockVfs` implements them in-memory (using `RwLock<HashMap>` for interior
mutability). `RealFs` delegates to `std::fs`. `LayeredVfs` forwards writes to `RealFs`.

`MkdirProcess`, `TouchProcess`, and `RmProcess` migrated from direct `std::fs` calls to
`self.vfs`. These three commands now accept a `vfs: Arc<dyn Vfs>` field and their tests
use `MockVfs` instead of real temp directories.

### `context_process.rs` uses `clank-ask` config (Q24)

`load_summarize_config()` and `SummarizeConfig` were a duplicate implementation of config
loading that had already drifted from the canonical version in `clank-ask`. Removed. `clank-ask`
added as a `[dependencies]` entry in `clank-shell/Cargo.toml`. `ContextProcess::with_config`
accepts `AskConfig` directly for test injection.

### `ClankShell::drop` deregisters global map entries (Q25)

Dispatch table, transcript table, and sudo state entries were never removed. In a test suite
with 50+ `ClankShell` instances this was unbounded growth. `impl Drop for ClankShell` calls
`deregister_all(self.shell_id)`.

### `run_interactive` non-blocking stdin (Q26)

`stdin.lock().read_line()` was called directly inside an `async fn`, blocking the Tokio
executor thread for the duration of every line read. Wrapped in `tokio::task::spawn_blocking`.

### Typed error enums in `ask_process.rs` (Q23)

`AskFlags::parse` returned `Result<_, String>`. `select_provider` returned
`Result<_, String>`. Replaced with `AskFlagError` and `ProviderSelectError` typed enums.
Callers can now match on error kind; the conversion to stderr text happens at the
`run_ask` boundary.

### `MockHttpClient` uses `tokio::sync::Mutex` (Q21)

Changed from `std::sync::Mutex` to `tokio::sync::Mutex`. The compiler now enforces that
lock guards are not held across `.await` points — previously this was a silent invariant.
All `.lock().unwrap()` call sites updated to `.lock().await`.

---

## Test quality fixes

- `#[tokio::test]` in 7 command test modules (was `Runtime::new().block_on()`).
- `test_ask_config_missing_file_returns_error` now tests `AskConfig::load`, not `std::fs`.
- `test_context_summarize_parse_failure` tightened from `exit == 0 || exit == 1` to
  `exit == 0` + `stdout.trim().is_empty()`.
- `SecretsRegistry::remove` moved before assertions in env_cmd test (cleanup-before-assert).
- `test_select_openai_compat_missing_url_returns_error` changed from `async fn` to `fn`.
- Missing stdout/stderr assertions added to system tests per AGENTS.md requirement.

---

## Documentation fixes

- **AGENTS.md** golden file section replaced with scenario harness documentation.
- **AGENTS.md** target contradiction fixed: "native (current) and `wasm32-wasip2` (deferred)".
