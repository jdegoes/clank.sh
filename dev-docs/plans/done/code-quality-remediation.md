---
title: "Plan: Code quality remediation"
date: 2026-03-06
author: agent
issue:
  - "dev-docs/issues/open/code-quality-remediation.md"
  - "dev-docs/issues/open/rust-quality-deferred.md"
---

# Plan: Code quality remediation

## Originating Issues

- `dev-docs/issues/open/code-quality-remediation.md` — correctness bugs, idiom violations,
  test quality issues, and stale documentation identified in the primary audit.
- `dev-docs/issues/open/rust-quality-deferred.md` — items initially excluded from the
  primary plan on review assessed as having clear, bounded fixes worth doing now.

## Guiding principle

Every change in this plan fixes a real problem with real consequences. Changes that are
purely stylistic without a correctness or clarity payoff are not included.

---

## Correctness bugs

### Q1 — `env` command is broken in production

**File:** `crates/clank-shell/src/builtins.rs:311`

`dispatch_builtin` passes `env: HashMap::new()` to every `ProcessContext`. `EnvProcess`
reads `ctx.env` to produce output, so `env` always prints nothing from the shell.

**Fix:** In `dispatch_builtin`, populate `ctx.env` with the current environment snapshot,
filtered through `SecretsRegistry`. The snapshot logic already exists in
`shell.rs:172–175` for the `/proc/environ` handler; extract it into a shared helper and
call it from both sites.

```rust
// In dispatch_builtin, replace `env: HashMap::new()` with:
env: current_env_snapshot(),

// Helper (new, shared):
fn current_env_snapshot() -> HashMap<String, String> {
    let secrets = SecretsRegistry::snapshot();
    std::env::vars()
        .filter(|(k, _)| !secrets.contains(k))
        .collect()
}
```

**Test:** Add a Level 2 integration test in `tests/context.rs` (or a new `tests/env.rs`):
`test_env_command_shows_current_environment` — run `export TEST_VAR=hello && env`, assert
stdout contains `TEST_VAR=hello`. Also add `test_env_masks_secret_exports` — run
`export --secret TEST_SECRET=val && env`, assert stdout shows `TEST_SECRET=***` not `val`.

Note: the existing `test_env_masks_secret_variables` unit test works because it
directly populates `ctx.env`; it will continue to pass. The new Level 2 test covers the
production dispatch path that was previously untested.

---

### Q2 — `SUDO_STATE` authorization bypass

**File:** `crates/clank-shell/src/shell.rs:15`

`SUDO_STATE` is a process-wide `AtomicBool`. Two paths can cause unauthorized elevation:

1. **Concurrent tests:** test A calls `sudo rm`, sets `SUDO_STATE = true`; test B's
   `SudoOnly` command dispatches before test A clears it; test B's command runs authorized.

2. **Parse error between set and clear:** `run_line` stores `true` on line 212, but the
   clear happens inside `dispatch_builtin`. If Brush returns a parse error before
   dispatching, the flag stays set for the next command.

**Fix:** Make sudo state per-shell-instance by threading it through the same
`ACTIVE_SHELL_ID` mechanism used for dispatch. Add a `SUDO_STATE` map keyed by
`shell_id` to `builtins.rs`:

```rust
// builtins.rs — alongside DISPATCH and TRANSCRIPTS
static SUDO_STATE: LazyLock<RwLock<HashMap<u64, bool>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

pub fn set_sudo_state(shell_id: u64, val: bool) {
    SUDO_STATE.write().unwrap().insert(shell_id, val);
}

pub fn get_sudo_state(shell_id: u64) -> bool {
    *SUDO_STATE.read().unwrap().get(&shell_id).unwrap_or(&false)
}
```

Replace `SUDO_STATE: AtomicBool` in `shell.rs` with calls to `set_sudo_state` /
`get_sudo_state`. Ensure the clear happens in `run_line` (after `run_string` returns),
not inside `dispatch_builtin`, so parse errors cannot leave the state set.

**Test:** Add to `tests/authorization.rs`:

- `test_sudo_state_cleared_after_command` — run `sudo rm /nonexistent` (exits 1),
  then run `rm /nonexistent` (must exit 5, not 1 — flag must be cleared).
- `test_sudo_state_not_shared_across_shells` — two `ClankShell` instances; set sudo
  in one, verify it does not affect the other.

---

### Q3 — `ExitShell` path truncates exit code to `u8`

**File:** `crates/clank-shell/src/shell.rs:302`

```rust
std::process::exit(u8::from(r.exit_code) as i32);
```

`u8::from` on a `brush_core::ExecutionExitCode` calls the `From` impl which extracts the
inner `u8` from `Custom(u8)`. This truncates any exit code above 255 to wrap modulo 256,
so `exit 256` becomes `exit 0`. The same bug was fixed in `dispatch_builtin` with
`.clamp(1, 255)`.

**Fix:** Extract a helper that maps `ExecutionExitCode` to `i32` consistently and use it
in both places:

```rust
// In builtins.rs or shell.rs (shared):
pub(crate) fn exit_code_to_i32(code: brush_core::ExecutionExitCode) -> i32 {
    match code {
        brush_core::ExecutionExitCode::Success => 0,
        brush_core::ExecutionExitCode::Custom(n) => n as i32,
        _ => 1,
    }
}
```

For the `ExitShell` path in `shell.rs`, use `r.exit_code` directly:
```rust
std::process::exit(exit_code_to_i32(r.exit_code));
```

This ensures `exit 256` from a script calls `std::process::exit(1)` (clamped by
`Custom(u8)`) rather than `exit(0)`.

**Note:** The truncation to `u8` happens at the `Custom(u8)` enum variant in brush-core,
not in our code — there is no way to preserve exit codes above 255 through Brush.
The fix ensures the mapping is at least consistent and documented, not two different
implementations.

---

## Rust idiom fixes

### Q4 — `grep` combined short flags do not work

**File:** `crates/clank-shell/src/commands/grep.rs:16–26`

```rust
let recursive = ctx.argv.iter().any(|a| a == "-r" || a == "-R");
let show_line_numbers = ctx.argv.iter().any(|a| a == "-n");
```

Combined flags like `grep -rn` or `grep -ni` are not recognized because the check is an
exact string match. This is user-visible: `grep -rn pattern dir` silently applies no flags.

**Fix:** Replace exact flag matching with a function that also checks for the flag
character within multi-character short-flag strings:

```rust
fn has_flag(argv: &[String], short: char, long: Option<&str>) -> bool {
    argv.iter().any(|a| {
        // Exact match: "-r", "-R", "--recursive"
        if let Some(l) = long { if a == l { return true; } }
        // Short flag: "-r" or combined "-rn", "-nr", etc. (but not "--recursive")
        if a.starts_with('-') && !a.starts_with("--") {
            return a[1..].contains(short);
        }
        false
    })
}
```

Apply to `recursive`, `show_line_numbers`, `files_only`, `ignore_case` in `grep.rs`, and
to `show_all`, `long` in `ls.rs`, and `recursive`, `force` in `rm.rs`.

**Tests to update:** The existing tests use single flags and continue to pass. Add:
- `test_grep_combined_flags_rn` — `grep -rn pattern /dir` applies both `-r` and `-n`
- `test_ls_combined_la` — `ls -la /dir` applies both `-l` and `-a`
- `test_rm_combined_rf` — `rm -rf /dir` removes recursively and suppresses errors

---

### Q5 — `pattern.to_lowercase()` recomputed per line in grep

**File:** `crates/clank-shell/src/commands/grep.rs:113`

```rust
let matches = if ignore_case {
    line.to_lowercase().contains(&pattern.to_lowercase())
```

`pattern.to_lowercase()` is O(pattern_len) and runs once per line. For a 10,000-line
file with a 10-character pattern, this is 10,000 redundant allocations.

**Fix:** Compute `pattern_lower` once before the loop in `grep_content`:

```rust
fn grep_content(content: &str, pattern: &str, ..., ignore_case: bool, ...) -> bool {
    let pattern_lower = if ignore_case {
        Some(pattern.to_lowercase())
    } else {
        None
    };
    for (i, line) in content.lines().enumerate() {
        let matches = match &pattern_lower {
            Some(p) => line.to_lowercase().contains(p.as_str()),
            None => line.contains(pattern),
        };
        ...
    }
}
```

---

### Q6 — `grep_recursive` takes `&Arc<dyn Vfs>` instead of `&dyn Vfs`

**File:** `crates/clank-shell/src/commands/grep.rs:142`

```rust
fn grep_recursive(vfs: &Arc<dyn Vfs>, ...) -> bool {
```

The function does not clone the Arc; it only calls methods through the reference.
`&dyn Vfs` is the correct signature.

**Fix:** Change to `vfs: &dyn Vfs` and update the call site from `&self.vfs` to
`self.vfs.as_ref()`.

---

### Q7 — `let mut ctx = ctx` should be `mut ctx` parameter

**File:** `crates/clank/src/processes.rs:25, 80`

```rust
async fn run(&self, ctx: ProcessContext) -> ProcessResult {
    let mut ctx = ctx;
```

The idiomatic form is:

```rust
async fn run(&self, mut ctx: ProcessContext) -> ProcessResult {
```

---

### Q8 — Default model string should be a named constant

**Files:** `config.rs`, `model_process.rs`, `context_process.rs`

`"anthropic/claude-sonnet-4-5"` appears as a literal in four places. Define once:

```rust
// crates/clank-ask/src/config.rs
pub const DEFAULT_MODEL: &str = "anthropic/claude-sonnet-4-5";
```

Use `DEFAULT_MODEL` everywhere the literal currently appears.

---

### Q9 — `ProcHandler::find_proc` clones entire process list

**File:** `crates/clank-vfs/src/proc_handler.rs:49–55`

```rust
fn procs(&self) -> Vec<ProcessSnapshot> {
    self.process_source.read().unwrap().clone()
}

fn find_proc(&self, pid: u64) -> Option<ProcessSnapshot> {
    self.procs().into_iter().find(|p| p.pid == pid)
}
```

For a process table with 50 entries each carrying a full environment snapshot, this
allocates O(50 × N_env_vars) on every VFS call (`/proc/<pid>/cmdline`, `/proc/<pid>/status`,
etc.).

**Fix:**

```rust
fn find_proc(&self, pid: u64) -> Option<ProcessSnapshot> {
    self.process_source
        .read()
        .expect("process source lock poisoned")
        .iter()
        .find(|p| p.pid == pid)
        .cloned()
}
```

Remove the `procs()` method entirely; its only caller was `find_proc` and `read_dir`
which can inline the read lock:

```rust
// read_dir /proc root:
let procs: Vec<_> = self.process_source.read().expect("poisoned").clone();
```

For `read_dir(/proc)`, cloning the whole list is unavoidable since we need all PIDs.
For all single-PID lookups, the optimized `find_proc` avoids it.

---

### Q10 — `resolve_model` unnecessary clone

**File:** `crates/clank-ask/src/config.rs:127–131`

```rust
pub fn resolve_model(&self, explicit: Option<&str>) -> String {
    explicit
        .map(str::to_string)
        .or_else(|| self.default_model.clone())
        .unwrap_or_else(|| "anthropic/claude-sonnet-4-5".to_string())
}
```

`self.default_model.clone()` unconditionally clones the `Option<String>` even when
`explicit` is `Some`. Rewrite using `as_deref()`:

```rust
pub fn resolve_model(&self, explicit: Option<&str>) -> String {
    explicit
        .or(self.default_model.as_deref())
        .unwrap_or(DEFAULT_MODEL)
        .to_string()
}
```

---

### Q11 — `recorded_requests()` on `MockHttpClient` should be renamed

**File:** `crates/clank-http/src/lib.rs:147–149`

```rust
pub fn recorded_requests(&self) -> Vec<Request> {
    self.requests.lock().unwrap().drain(..).collect()
}
```

An inspection function named `recorded_requests()` should be non-destructive. Calling it
twice returns an empty vec on the second call, which is surprising.

**Fix:** Rename to `take_recorded_requests()` to make the destructive semantics explicit.
Update all call sites (currently none exist in the codebase — all tests access
`mock.requests` directly).

---

### Q12 — URL trailing-slash handling in providers

**Files:** `ollama.rs:104`, `openrouter.rs:84`, `openai_compat.rs:96`, `anthropic.rs:114`

```rust
url: format!("{}/api/chat", self.base_url),
```

If `base_url` has a trailing slash, this produces a double-slash URL
(`http://localhost:11434//api/chat`). Most servers handle this but it is incorrect.

**Fix:** Trim trailing slashes from `base_url` at construction time in each provider's
`new()`:

```rust
pub fn new(base_url: impl Into<String>, http: Arc<dyn HttpClient>) -> Self {
    Self {
        base_url: base_url.into().trim_end_matches('/').to_string(),
        ...
    }
}
```

**Test:** Add `test_ollama_trailing_slash_in_base_url_is_normalized` and equivalents for
each provider — verify that a `base_url` with a trailing slash produces a correct URL
without double-slash.

---

### Q13 — `init_global_registry` lacks idempotency guard

**File:** `crates/clank-manifest/src/lib.rs:177`

`init_global_registry()` is called from every `ClankShell::new()` and unconditionally
re-registers all default commands. In a test suite that creates 30+ shells, this is
30+ redundant re-registrations.

**Fix:** Use `OnceLock` to ensure single initialization:

```rust
static REGISTRY_INIT: std::sync::OnceLock<()> = std::sync::OnceLock::new();

pub fn init_global_registry() {
    REGISTRY_INIT.get_or_init(|| {
        GLOBAL_REGISTRY.write().unwrap().populate_defaults();
    });
}
```

---

## Test quality fixes

### Q14 — Replace `Runtime::new().block_on()` with `#[tokio::test]` in 7 command test modules

**Files:** `grep.rs`, `ls.rs`, `mkdir.rs`, `rm.rs`, `touch.rs`, `stat_cmd.rs`, `env_cmd.rs`

All 7 command test modules create a Tokio runtime manually with
`tokio::runtime::Runtime::new().unwrap().block_on(...)` inside a `#[test]` function.
AGENTS.md requires `#[tokio::test]`.

**Fix:** In each file, change the `run` helper to `async fn run(...)` returning the output
tuple, change `#[test]` to `#[tokio::test]`, and call `run(...).await` directly.

Since `Process::run` is `async`, this is the natural form. The current `block_on` wrapper
only exists to make an async fn work inside a sync test — removing the need for it.

---

### Q15 — Fix `test_ask_config_missing_file_returns_error`

**File:** `crates/clank-ask/src/config.rs:197–202`

Replace the vacuous stdlib test with one that actually tests `AskConfig::load`:

```rust
#[test]
fn test_ask_config_load_returns_not_found_for_missing_file() {
    let _lock = ENV_LOCK.lock().unwrap();
    std::env::set_var("CLANK_CONFIG", "/nonexistent/path/ask.toml");
    let result = AskConfig::load();
    std::env::remove_var("CLANK_CONFIG");
    assert!(
        matches!(result, Err(ConfigError::NotFound { .. })),
        "expected ConfigError::NotFound, got: {result:?}"
    );
}
```

---

### Q16 — Tighten `test_context_summarize_parse_failure` assertion

**File:** `crates/clank-shell/tests/context.rs:296–303`

The current `exit_code == 0 || exit_code == 1` assertion is vacuous. The actual
deterministic behaviour when the JSON has no `content` array is exit 0 with empty stdout:

```rust
let result = proc.run(ctx).await;
assert_eq!(result.exit_code, 0, "missing content array must exit 0 gracefully");
let stdout = std::fs::read_to_string(out.path()).unwrap();
assert!(stdout.is_empty(), "no summary text must be emitted: {stdout}");
```

---

### Q17 — Fix teardown-after-assert in `env_cmd` test

**File:** `crates/clank-shell/src/commands/env_cmd.rs`

Move `SecretsRegistry::remove("MY_SECRET")` to before the assertion, or add it in a
scope that ensures cleanup even on panic:

```rust
#[tokio::test]
async fn test_env_masks_secret_variables() {
    SecretsRegistry::insert("MY_SECRET");
    let mut env = HashMap::new();
    env.insert("MY_SECRET".to_string(), "super-secret-value".to_string());
    let (stdout, code) = run(env).await;

    // Clean up before asserting so a failing assert doesn't leave the secret registered.
    SecretsRegistry::remove("MY_SECRET");

    assert_eq!(code, 0);
    assert!(stdout.contains("MY_SECRET=***"), "secret must be masked: {stdout}");
    assert!(!stdout.contains("super-secret-value"), "plaintext must not appear: {stdout}");
}
```

Apply the same pattern to `test_env_prints_non_secret_variables_plaintext` — though it
doesn't insert any secrets, it should use the `#[tokio::test]` form after Q14 is applied.

---

### Q18 — Add missing stdout assertions to system tests

**File:** `crates/clank/tests/ask.rs`

AGENTS.md requires asserting both stdout and stderr. Add stdout assertions:

- `test_ask_no_config_exits_with_message`: add `.stdout(predicate::str::is_empty())` — an
  error must not produce stdout.
- `test_ask_bad_args_exits_stderr`: add `.stdout(predicate::str::is_empty())`.
- `test_model_no_subcommand`: add `.stdout(predicate::str::is_empty())`.
- `test_context_clear_succeeds`: add `.stderr(predicate::str::is_empty())`.

---

### Q19 — `test_select_openai_compat_missing_url_returns_error` should be `#[test]` not `#[tokio::test]`

**File:** `crates/clank-ask/src/ask_process.rs`

`select_provider` is a synchronous function. The test has no `.await`. Change to `#[test]`
and `fn`.

---

## Documentation fixes

### Q20 — Update `AGENTS.md` golden test section

Replace the `trycmd`-based golden test documentation (lines 282–346) with documentation
for the actual YAML scenario harness. Key points to document:
- Fixture format (YAML with `stdin`, `stdout`, `stderr`, `config`, `config_after` keys)
- Location: `crates/clank/tests/scenarios/`
- Runner: `cargo test --test scenario`
- Regeneration: `CLANK_UPDATE=1 cargo test --test scenario`

Also fix the contradiction on line 55: change `wasm32-wasip2 (primary) and native (secondary)` to `native (current target) and wasm32-wasip2 (deferred — see project statement above)`.

---

## Previously deferred items (from `rust-quality-deferred.md`)

### Q21 — `MockHttpClient` should use `tokio::sync::Mutex`

**File:** `crates/clank-http/src/lib.rs:125–127`

`MockHttpClient` holds `std::sync::Mutex` guards inside an `async fn`. The critical
sections do not currently span `.await` points, but `std::sync::Mutex` in async code
is a latent hazard: the compiler does not prevent a future change from introducing an
`.await` while the guard is live, which would deadlock the executor. `tokio::sync::Mutex`
fails to compile in that scenario, making the constraint enforced rather than merely
documented.

**Fix:** Change both `std::sync::Mutex` fields on `MockHttpClient` to
`tokio::sync::Mutex`. Update `send()` to use `.await` on lock acquisition. The public
`requests` field accessor in tests will need `.lock().await` instead of `.lock().unwrap()`.

Since `MockHttpClient` is test infrastructure used exclusively in async test contexts,
the ergonomic cost is zero and the safety benefit is real.

---

### Q22 — Replace `.unwrap()` on lock acquisitions with `.expect()` throughout production code

**Files:** All non-test code in `clank-shell/src/`, `clank-vfs/src/`, `clank-ask/src/`,
`clank/src/`

AGENTS.md states "No `unwrap()` or `expect()` outside tests." While `.expect()` also
panics, it is explicitly sanctioned for "this is a programming error" situations and
produces a diagnostic message that identifies the failure site. Bare `.unwrap()` produces
no context.

**Fix:** A systematic pass replacing every `.unwrap()` on a lock acquisition in non-test
production code with `.expect("<description> poisoned")`. This is purely mechanical and
does not change behaviour, but it makes panics diagnosable and satisfies the AGENTS.md
convention.

Scope: the 84 `.unwrap()` calls in `clank-shell/src/` (not in `#[cfg(test)]` blocks),
plus similar patterns in `clank-vfs` and `clank-ask`. Instances in `#[test]` code are
intentionally left as `.unwrap()` (AGENTS.md allows this in tests).

---

### Q23 — Replace `Result<_, String>` with typed errors in `ask_process.rs`

**File:** `crates/clank-ask/src/ask_process.rs`

`AskFlags::parse` (line 57) and `select_provider` (line 227) return `Result<_, String>`.
AGENTS.md: "Error types are typed enums with distinct variants — never stringly typed."

**Fix:** Introduce two small error enums:

```rust
#[derive(Debug, thiserror::Error)]
pub enum AskFlagError {
    #[error("unknown flag: {0}")]
    UnknownFlag(String),
    #[error("--model requires an argument")]
    ModelMissingArgument,
}

#[derive(Debug, thiserror::Error)]
pub enum ProviderSelectError {
    #[error("no base_url configured for provider '{0}'\nRun: model add {0} --url <URL>")]
    MissingBaseUrl(String),
    #[error("no API key configured for provider '{provider_name}' or 'openrouter'\n{hint}")]
    MissingApiKey { provider_name: String, hint: String },
}
```

`run_ask` converts these to `(String, Vec<u8>, i32)` at the boundary, as it does today.
Callers that currently format the `String` directly continue to work without change.
The benefit is that future callers (e.g., a JSON output mode or an MCP tool adapter) can
match on error kind rather than parsing strings.

---

### Q24 — `context_process.rs` should use `clank-ask` config, not re-implement it

**File:** `crates/clank-shell/src/context_process.rs:213–251`

`load_summarize_config()` duplicates config path resolution and TOML parsing that already
exists correctly in `clank-ask::config`. The duplicate has drifted: it uses
`unwrap_or_default()` on `dirs_next::config_dir()` (producing a relative path `ask/ask.toml`
on systems without a config dir) while `clank-ask::config_path()` uses `unwrap_or_else`.

Adding `clank-ask` as a dependency of `clank-shell` is non-circular: neither depends on
the other currently. Both depend on `clank-http`. The dependency graph after this change:

```
clank-ask  ──→  clank-http
clank-shell ──→ clank-http, clank-ask (new)
clank ──→ clank-shell, clank-ask
```

**Fix:**
1. Add `clank-ask = { workspace = true }` to `clank-shell/Cargo.toml`.
2. Replace `load_summarize_config()` with `clank_ask::config::AskConfig::load_or_default()`.
3. Replace `SummarizeConfig { api_key, model }` with `clank_ask::config::AskConfig` as
   the injected type in `ContextProcess::with_config()`. The production `new()` constructor
   loads from disk using `AskConfig::load_or_default()`.
4. Update `select_provider`-equivalent logic in `summarize` to use
   `AskConfig::api_key("anthropic")` directly, with the same error message as before.
5. Remove `load_summarize_config()`, `SummarizeConfig`, and the manual TOML parsing.
6. Update `context.rs` tests: `SummarizeConfig` becomes `AskConfig`.

This also eliminates the `DEFAULT_MODEL` duplication between crates — `context_process.rs`
can use `clank_ask::config::DEFAULT_MODEL` directly.

---

### Q25 — `ClankShell::drop` should deregister dispatch table entries

**File:** `crates/clank-shell/src/shell.rs` / `crates/clank-shell/src/builtins.rs`

`DISPATCH` and `TRANSCRIPTS` are global HashMaps that accumulate one entry per
`ClankShell` instance and never clean up. In a test run with 50+ `ClankShell` instances,
this is unbounded growth of static memory.

**Fix:** Implement `Drop` for `ClankShell`:

```rust
impl Drop for ClankShell {
    fn drop(&mut self) {
        // Remove all dispatch entries registered for this shell.
        if let Ok(mut guard) = DISPATCH.write() {
            if let Some(table) = guard.as_mut() {
                table.retain(|(id, _), _| *id != self.shell_id);
            }
        }
        if let Ok(mut guard) = TRANSCRIPTS.write() {
            if let Some(table) = guard.as_mut() {
                table.remove(&self.shell_id);
            }
        }
    }
}
```

Export `deregister_all` from `builtins.rs` and call it from `Drop`. Also clean up the
`SUDO_STATE` map entry for the shell (added in Q2).

---

### Q26 — `run_interactive` should not block the Tokio executor thread

**File:** `crates/clank-shell/src/shell.rs:333–377`

`stdin.lock().read_line()` is a blocking syscall called directly inside an `async fn`.
In Tokio's single-threaded runtime, this blocks the executor thread for each line read,
preventing any async work (timers, I/O completion, etc.) from running while the user is
typing.

**Fix:** Wrap the blocking read in `tokio::task::spawn_blocking`:

```rust
let line = tokio::task::spawn_blocking(move || {
    let mut line = String::new();
    let n = std::io::stdin().lock().read_line(&mut line)?;
    Ok::<(String, usize), std::io::Error>((line, n))
})
.await
.expect("spawn_blocking panicked")?;
```

This moves the blocking call onto Tokio's blocking thread pool, freeing the async
executor for other work while waiting for user input. It is the standard Tokio pattern
for wrapping blocking stdlib I/O.

---

### Q27 — `Vfs` trait must expose write operations; `mkdir`, `touch`, `rm` must use them

**Files:** `crates/clank-vfs/src/lib.rs`, `mkdir.rs`, `touch.rs`, `rm.rs`

`mkdir`, `touch`, and `rm` bypass the `Vfs` abstraction and call `std::fs` directly.
This breaks testability (their tests must hit the real filesystem), WASM portability, and
the VFS layering model.

**Fix — `Vfs` trait additions:**

```rust
pub trait Vfs: Send + Sync {
    // existing read operations ...

    fn write_file(&self, path: &Path, contents: &[u8]) -> Result<(), VfsError>;
    fn create_dir(&self, path: &Path) -> Result<(), VfsError>;
    fn create_dir_all(&self, path: &Path) -> Result<(), VfsError>;
    fn remove_file(&self, path: &Path) -> Result<(), VfsError>;
    fn remove_dir_all(&self, path: &Path) -> Result<(), VfsError>;
}
```

**Fix — `MockVfs` implementation** (in-memory, using the existing `files: HashMap`):

```rust
fn write_file(&self, path: &Path, contents: &[u8]) -> Result<(), VfsError> {
    self.files.insert(path.to_owned(), contents.to_vec());
    Ok(())
}
fn create_dir(&self, _path: &Path) -> Result<(), VfsError> { Ok(()) }
fn create_dir_all(&self, _path: &Path) -> Result<(), VfsError> { Ok(()) }
fn remove_file(&self, path: &Path) -> Result<(), VfsError> {
    self.files.remove(path)
        .map(|_| ())
        .ok_or_else(|| VfsError::NotFound(path.to_owned()))
}
fn remove_dir_all(&self, path: &Path) -> Result<(), VfsError> {
    let prefix = path.to_owned();
    self.files.retain(|k, _| !k.starts_with(&prefix));
    Ok(())
}
```

**Fix — `RealFs` implementation:** Delegates to `std::fs` (as it does for reads).

**Fix — `MkdirProcess`:** Replace `std::fs::create_dir`/`create_dir_all` with
`self.vfs.create_dir`/`create_dir_all`. Add `pub vfs: Arc<dyn Vfs>` field.

**Fix — `TouchProcess`:** Replace `std::fs::OpenOptions`/`File::create` with
`self.vfs.write_file(path, b"")` for new files and `vfs.stat` for existence check.
Add `pub vfs: Arc<dyn Vfs>` field.

**Fix — `RmProcess`:** Replace `std::fs::remove_file`/`remove_dir_all` with
`self.vfs.remove_file`/`remove_dir_all`. Add `pub vfs: Arc<dyn Vfs>` field.

**Fix — Registration in `shell.rs`:** Pass `Arc::clone(&vfs)` to `MkdirProcess`,
`TouchProcess`, and `RmProcess` constructors (same pattern as `LsProcess`, `CatProcess`).

**Fix — Tests:** Update `mkdir.rs`, `touch.rs`, `rm.rs` unit tests to use `MockVfs`
instead of real tempfiles. The tests become faster and isolated.

**Note on `MockVfs` mutability:** The write methods require interior mutability since
`Vfs::write_file` takes `&self`. Wrap `files` in `std::sync::RwLock<HashMap<...>>` in
`MockVfs`. This is consistent with how `MockHttpClient` handles its request log.

---

## Changed files

| File | Changes |
|---|---|
| `crates/clank-shell/src/builtins.rs` | Q1: populate `ctx.env`; Q2: per-shell sudo state; Q25: `Drop` cleanup |
| `crates/clank-shell/src/shell.rs` | Q2: use per-shell sudo state; Q3: fix ExitShell exit code; Q25: `Drop` impl; Q26: non-blocking stdin |
| `crates/clank-shell/src/commands/grep.rs` | Q4: `has_flag()`; Q5: pattern_lower once; Q6: `&dyn Vfs`; Q14: `#[tokio::test]` |
| `crates/clank-shell/src/commands/ls.rs` | Q4: `has_flag()`; Q14: `#[tokio::test]` |
| `crates/clank-shell/src/commands/rm.rs` | Q4: `has_flag()`; Q14: `#[tokio::test]`; Q27: use `Vfs` |
| `crates/clank-shell/src/commands/mkdir.rs` | Q14: `#[tokio::test]`; Q27: use `Vfs` |
| `crates/clank-shell/src/commands/touch.rs` | Q14: `#[tokio::test]`; Q27: use `Vfs` |
| `crates/clank-shell/src/commands/env_cmd.rs` | Q14: `#[tokio::test]`; Q17: teardown before assert |
| `crates/clank-shell/src/commands/stat_cmd.rs` | Q14: `#[tokio::test]` |
| `crates/clank-shell/src/context_process.rs` | Q24: use `clank-ask` config; remove `load_summarize_config` |
| `crates/clank-shell/src/secrets.rs` + all non-test production code | Q22: `.unwrap()` → `.expect()` pass |
| `crates/clank-shell/tests/context.rs` | Q16: tighten parse failure assertion; Q2: sudo isolation tests; Q24: update injected type |
| `crates/clank-shell/tests/authorization.rs` | Q2: add sudo state isolation tests |
| `crates/clank-shell/tests/env.rs` (new) | Q1: production env dispatch tests |
| `crates/clank-shell/Cargo.toml` | Q24: add `clank-ask` dependency |
| `crates/clank-vfs/src/lib.rs` | Q27: add write operations to `Vfs` trait and `MockVfs`; `RwLock` for `MockVfs.files` |
| `crates/clank-vfs/src/proc_handler.rs` | Q9: optimize `find_proc` |
| `crates/clank-ask/src/config.rs` | Q8: `DEFAULT_MODEL` constant; Q10: `resolve_model` cleanup; Q15: fix vacuous test |
| `crates/clank-ask/src/model_process.rs` | Q8: use `DEFAULT_MODEL` |
| `crates/clank-ask/src/ask_process.rs` | Q19: `async → sync` test; Q23: typed `AskFlagError`/`ProviderSelectError` |
| `crates/clank-ask/src/provider/ollama.rs` | Q12: trim trailing slash |
| `crates/clank-ask/src/provider/openrouter.rs` | Q12: trim trailing slash |
| `crates/clank-ask/src/provider/openai_compat.rs` | Q12: trim trailing slash |
| `crates/clank-ask/src/provider/anthropic.rs` | Q12: trim trailing slash |
| `crates/clank-http/src/lib.rs` | Q11: rename `recorded_requests`; Q21: `tokio::sync::Mutex` |
| `crates/clank-manifest/src/lib.rs` | Q13: `OnceLock` idempotency |
| `crates/clank/src/processes.rs` | Q7: `mut ctx` parameter |
| `crates/clank/tests/ask.rs` | Q18: add missing stdout assertions |
| `AGENTS.md` | Q20: update scenario harness docs; fix target contradiction |

---

## Acceptance criteria

1. `cargo test --workspace` passes with zero failures.
2. `cargo clippy --all-targets -- -D warnings` passes.
3. `cargo fmt --check` passes.
4. `env` run from the clank shell produces the current environment, with secrets masked.
5. A `SudoOnly` command after a failed `sudo` invocation is correctly denied (SUDO_STATE
   cleared on every `run_line` exit, not only on successful dispatch).
6. Two `ClankShell` instances in the same process do not share sudo authorization state.
7. `grep -rn`, `ls -la`, `rm -rf` with combined flags work correctly.
8. `AGENTS.md` no longer references `trycmd`, `golden.rs`, or `tests/fixtures/`.
9. No bare `.unwrap()` remains in non-test production code outside `#[cfg(test)]` blocks.
10. `mkdir`, `touch`, and `rm` tests no longer require a real filesystem.
11. `context summarize` uses `clank-ask::config::AskConfig` for config loading; no
    duplicate config path resolution exists.

---

## Tasks

### Correctness bugs
- [ ] **Q1** Fix `env` command: populate `ctx.env` from `current_env_snapshot()` in `dispatch_builtin`; add Level 2 tests in `tests/env.rs`
- [ ] **Q2** Fix `SUDO_STATE`: per-shell map in `builtins.rs`; clear in `run_line` unconditionally; add isolation tests in `authorization.rs`
- [ ] **Q3** Fix `ExitShell` exit code: extract `exit_code_to_i32` helper; use in both `shell.rs` sites

### Rust idiom fixes
- [ ] **Q4** Combined short flags: `has_flag()` helper in `grep.rs`, `ls.rs`, `rm.rs`; add combined-flag tests
- [ ] **Q5** Grep case-insensitive: compute `pattern_lower` once before loop
- [ ] **Q6** `grep_recursive`: `&Arc<dyn Vfs>` → `&dyn Vfs`
- [ ] **Q7** `processes.rs`: `let mut ctx = ctx` → `mut ctx` parameter
- [ ] **Q8** `DEFAULT_MODEL` constant in `config.rs`; replace all four literal occurrences
- [ ] **Q9** `ProcHandler::find_proc`: eliminate full-list clone; remove `procs()` method
- [ ] **Q10** `resolve_model`: `or(as_deref()).unwrap_or(DEFAULT_MODEL).to_string()`
- [ ] **Q11** `MockHttpClient::recorded_requests` → `take_recorded_requests`
- [ ] **Q12** Provider `new()` constructors: `trim_end_matches('/')` on `base_url`; add normalization tests
- [ ] **Q13** `init_global_registry`: `OnceLock` idempotency guard

### Test quality fixes
- [ ] **Q14** Replace `Runtime::new().block_on()` with `#[tokio::test]` in 7 command test modules
- [ ] **Q15** `test_ask_config_missing_file_returns_error`: test `AskConfig::load`, not `std::fs`
- [ ] **Q16** `test_context_summarize_parse_failure`: assert `exit_code == 0` + `stdout.is_empty()`
- [ ] **Q17** `env_cmd` tests: `SecretsRegistry::remove` before assertions, not after
- [ ] **Q18** System tests: add `stdout(is_empty())` to 3 tests; `stderr(is_empty())` to 2
- [ ] **Q19** `test_select_openai_compat_missing_url_returns_error`: `async fn` → `fn`

### Documentation fixes
- [ ] **Q20** `AGENTS.md`: replace trycmd section with scenario harness docs; fix target contradiction

### Previously deferred items
- [ ] **Q21** `MockHttpClient`: `std::sync::Mutex` → `tokio::sync::Mutex`; update all `.lock().unwrap()` to `.lock().await`
- [ ] **Q22** `.unwrap()` → `.expect()` pass over all non-test production code in `clank-shell`, `clank-vfs`, `clank-ask`, `clank`
- [ ] **Q23** `ask_process.rs`: introduce `AskFlagError` and `ProviderSelectError` typed error enums; remove `Result<_, String>` from `parse()` and `select_provider()`
- [ ] **Q24** `context_process.rs`: add `clank-ask` dep to `clank-shell`; replace `load_summarize_config` + `SummarizeConfig` with `AskConfig`; update tests
- [ ] **Q25** `ClankShell::drop`: remove dispatch, transcript, and sudo state entries for `self.shell_id`
- [ ] **Q26** `run_interactive`: wrap `stdin.lock().read_line()` in `tokio::task::spawn_blocking`
- [ ] **Q27** `Vfs` trait: add write operations; `MockVfs`: implement in-memory with `RwLock`; migrate `mkdir`, `touch`, `rm` to use `self.vfs`; update tests to use `MockVfs`

### Quality gate
- [ ] **QG** `cargo test --workspace`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check` all pass; acceptance criteria 4–11 verified
