---
title: "Plan: Phase 2 Deviation Remediation"
date: 2026-03-06
author: agent
issue: "dev-docs/issues/open/phase-2-deviations.md"
research:
  - "dev-docs/research/phase-2-deviation-remediation.md"
designs:
  - "dev-docs/designs/approved/workspace-and-crate-structure-realized.md"
---

# Plan: Phase 2 Deviation Remediation

## Originating Issue

`dev-docs/issues/open/phase-2-deviations.md` — seven deviations from spec identified
after Phase 2 implementation.

## Research Consulted

`dev-docs/research/phase-2-deviation-remediation.md` — confirms `ExecutionContext` has
`pub shell: &mut Shell`; identifies the correct approach for each fix; establishes PID
threading via `ProcessContext.pid`; confirms `rpassword` for echo suppression.

## Developer Feedback

No open design questions. All approaches confirmed by research.

## Approach

Seven targeted fixes. Ordered by severity — the three significant fixes first (1–4), then
the minor ones (5–7).

---

## Tasks

### Dev 1: `export --secret` — perform real env mutation

- [ ] Add `declaration_builtin: true` to the `export` Registration in `clank_builtins()`
- [ ] In `dispatch_builtin`, add a special-case branch for `cmd_name == "export"`:
  - Parse args to find `CommandArg::Assignment` entries
  - For each assignment: call `context.shell.env.update_or_add(name, value, |v| { v.export(); Ok(()) }, EnvironmentLookup::Anywhere, EnvironmentScope::Global)`
  - If `--secret` is present in args, also call `SecretsRegistry::insert(name)`
  - Then proceed with normal dispatch (calls `ExportProcess::run` for any remaining logic)
- [ ] `ExportProcess::run` reduced to only the `SecretsRegistry` side effect (env mutation
  is now in `dispatch_builtin`)
- [ ] Unit test: after `export FOO=bar`, `std::env::var("FOO")` returns `"bar"`
  (via shell integration test using `run_line("export FOO=bar") && run_line("echo $FOO")`)
- [ ] Unit test: after `export --secret KEY=val`, `SecretsRegistry::contains("KEY")` is true
  and the variable value is accessible in the shell env

### Dev 2: `confirm` sets P state and uses proper prompt format

- [ ] Thread PID from `dispatch_builtin` into the authorization check so P state can be
  set on the correct process table entry (prerequisite for Dev 4 also)
- [ ] In the `Confirm` branch: call `process_table::set_status(shell_id, pid, Paused)`
  before presenting the confirmation prompt
- [ ] Replace inline `eprint!` + `read_line` with a `prompt-user`-style interaction:
  display the command name and the spec-matching format:
  `"<command> requires confirmation. (y)es, (n)o: "`
- [ ] Reset to `Running` after response
- [ ] Unit test: `Confirm` policy sets P state before prompting, R state after

### Dev 3: `sudo` prefix strips from dispatched line

- [ ] In `run_line()`, when `sudo` is detected as the first word: build `effective_line`
  by stripping `"sudo "` from the start, then pass `effective_line` to `run_string`
  instead of the original `line`
- [ ] Ensure the transcript records the original line (with `sudo`) not the stripped line
- [ ] Integration test: `sudo rm /tmp/test-file` works — `rm` runs, not `sudo rm`

### Dev 4: Thread PID through `ProcessContext`

- [ ] Add `pid: u64` field to `ProcessContext` in `process.rs`
- [ ] In `dispatch_builtin`: set `pid` on `ProcessContext` after calling
  `process_table::spawn(...)` 
- [ ] Update `StubProcess` and all other `Process` impls to receive `ctx.pid` if needed
- [ ] Update `PromptUserProcess::run` to use `ctx.pid` instead of `ACTIVE_SHELL_ID` for
  P state transitions
- [ ] Unit test: `PromptUserProcess` sets P state on `ctx.pid`, not on the shell ID

### Dev 5: `--secret` suppresses echo

- [ ] Add `rpassword = "7"` to `clank-shell` dependencies
- [ ] In `read_response()` (prompt_user.rs): when `secret == true`, call
  `rpassword::read_password()` instead of `stdin.lock().read_line()`
- [ ] Validate the password against `choices` the same way as a normal response
- [ ] Unit test: `--secret` flag causes `read_password()` to be used (mock/verify via
  integration test that the function path is taken)

### Dev 6: `ps aux` includes `%CPU`/`%MEM` columns

- [ ] Update `PsProcess::run` to emit the full standard column format:
  - `ps aux`: `USER PID %CPU %MEM VSZ RSS TTY STAT START TIME COMMAND`
    with `-` for all non-meaningful columns
  - `ps -ef`: `UID PID PPID C STIME TTY TIME CMD`
    with `-` for non-meaningful columns
- [ ] Unit test: `ps aux` output contains `%CPU` and `%MEM` headers

### Dev 7: `/proc/<pid>/environ` populated from real env

- [ ] In `shell.rs`, when building `ProcessSnapshot`, populate `environ` from
  `std::env::vars()`, filtering out any key present in `SecretsRegistry::snapshot()`
- [ ] Integration test: `cat /proc/<pid>/environ` returns non-empty content containing
  at least one well-known env var (e.g. `HOME` or `PATH`)

---

## Acceptance Tests

| # | Test | Location | Assertion |
|---|---|---|---|
| D1a | `test_export_sets_env_variable` | `clank-shell/tests/context.rs` | After `export FOO=bar`, `echo $FOO` returns `bar` |
| D1b | `test_export_secret_registers_in_secrets` | `clank-shell/tests/context.rs` | After `export --secret KEY=val`, `SecretsRegistry::contains("KEY")` is true |
| D2 | `test_confirm_sets_p_state` | `clank-shell/src/shell.rs` (unit) | `Confirm` policy entry sets status to `Paused` before prompting |
| D3 | `test_sudo_strips_prefix_from_dispatch` | `clank-shell/tests/context.rs` | `sudo rm /tmp/x` dispatches `rm`, not `sudo rm` |
| D4 | `test_prompt_user_uses_ctx_pid` | `clank-shell/src/commands/prompt_user.rs` (unit) | P state set on `ctx.pid`, not on shell ID |
| D5 | `test_secret_flag_uses_read_password` | `clank-shell/tests/context.rs` | `--secret` path calls rpassword (integration) |
| D6 | `test_ps_aux_has_cpu_mem_columns` | `clank-shell/tests/context.rs` | `ps aux` output contains `%CPU` and `%MEM` in header |
| D7 | `test_proc_environ_not_empty` | `clank-shell/tests/transcript_capture.rs` | `cat /proc/<pid>/environ` returns non-empty content |
