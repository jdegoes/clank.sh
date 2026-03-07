---
title: "Plan: Phase 2 — Process model, job control, authorization, prompt-user, VFS, core commands"
date: 2026-03-06
author: agent
issue: "dev-docs/issues/open/phase-2-process-model.md"
research:
  - "dev-docs/research/spec-analysis-and-implementation-gaps.md"
  - "dev-docs/research/virtual-filesystem-driver-options.md"
  - "dev-docs/research/brush-command-dispatch.md"
  - "dev-docs/research/phase-2-prerequisite-spikes.md"
designs:
  - "dev-docs/designs/approved/workspace-and-crate-structure-realized.md"
  - "dev-docs/designs/approved/phase-1-transcript-and-ask-realized.md"
---

# Plan: Phase 2 — Process model, job control, authorization, `prompt-user`, VFS, core commands

## Originating Issue

`dev-docs/issues/open/phase-2-process-model.md` (revised 2026-03-06).

## Research Consulted

- `dev-docs/research/virtual-filesystem-driver-options.md` — VFS approach selection
  (Approach C: VFS at command implementation layer)
- `dev-docs/research/brush-command-dispatch.md` — Brush builtin registration mechanics
- `dev-docs/research/phase-2-prerequisite-spikes.md` — Brush I/O hook spike (no hook in
  0.4.0; `/proc/` in redirections deferred); `export --secret` mechanism (override via
  registration, no fork needed)

## Developer Feedback

**Process table storage (2026-03-06):** Global `LazyLock<RwLock>` pattern confirmed.
`dispatch_builtin` is a bare fn pointer and cannot capture state — any process table
access from it must go through a global regardless. Making it explicit and well-typed
is the correct approach. No better alternative exists given the architectural constraint.

**Background execution (2026-03-06):** `AbortHandle` confirmed. Hard cancellation is
idiomatic for Tokio tasks, matches the spec's `kill` semantics, and requires no changes
to the `Process` trait. Cooperative cancellation via channels would require every
`Process::run()` implementer to poll a cancellation signal — unnecessary complexity.
Phase 4 replaces `abort()` with the Golem cancellation API for agent invocations.

**`prompt-user` Markdown rendering (2026-03-06):** Real Markdown rendering required in
Phase 2, not deferred. The spec is explicit: *"The terminal renders it as readable text —
tables, emphasis, links"* and *"A model can pipe a diff, a summary table, or a formatted
report into `prompt-user`."* A table rendered as unwrapped text is unreadable and defeats
the purpose of the interface. Use `termimad` (a lightweight ANSI Markdown renderer,
~3K lines, no heavy dependencies). This is not a Phase 5 nicety — it is a Phase 2
requirement.

## Approach

Phase 2 is the largest phase to date. It is broken into five self-contained streams that
can be implemented in order, each building on the last.

### Stream A: Process table

A global `ProcessTable` keyed by `(shell_id, pid)`. Every dispatched command is registered
before execution and updated on completion. PIDs are monotonically increasing per shell
instance. The existing `shell_id` mechanism is reused.

### Stream B: Core command implementations + VFS

`LayeredVfs` added to `clank-vfs`: a mount table of `PathBuf` prefix → `Box<dyn VfsHandler>`,
falling through to `RealFs` for unmounted paths. Initially only `/proc/` is mounted.

Every core command that was a stub gets a real implementation using the VFS:
`ls`, `cat`, `grep`, `stat`, `mkdir`, `rm`, `cp`, `mv`, `touch`, `env`.

This resolves transcript capture: once `ls` etc. go through the dispatch table and produce
output, that output is captured into the transcript by the existing tempfile mechanism.

### Stream C: `/proc/` virtual namespace

`ProcHandler` reads from the live `ProcessTable` on each access. `/proc/clank/system-prompt`
reads from the `ManifestRegistry`. No file writes; fully computed on read.

I/O redirect limitation: `cat < /proc/1/cmdline` will not work because Brush has no open
hook (see spike). This is documented as a known limitation. The upstream PR is filed in
parallel. `cat /proc/1/cmdline` (with cat as a registered builtin) works correctly.

### Stream D: `prompt-user` and `P` state

`PromptUserProcess` blocks on `std::io::stdin` and enters the `P` state in the process
table while waiting. The process table entry is updated to `R` when the user responds.
`--secret` responses are never written to the transcript.

### Stream E: Authorization model

`run_line()` consults the `ManifestRegistry` for the command's `authorization_policy`
before dispatching. `Confirm` calls `prompt-user` internally. `SudoOnly` rejects unless
the shell is in a `sudo`-authorized state. `export --secret` overrides Brush's export
builtin via `ClankExportProcess`.

---

## Tasks

### Stream A: Process table

- [ ] Define `ProcessEntry` in `clank-shell/src/process_table.rs`:
  `pid: u64`, `ppid: u64`, `shell_id: u64`, `type_tag: ProcessType`, `argv: Vec<String>`,
  `status: ProcessStatus`, `start_time: SystemTime`,
  `join_handle: Option<tokio::task::AbortHandle>`
- [ ] Define `ProcessStatus` enum: `Running`, `Sleeping`, `Suspended`, `Zombie`, `Paused`
- [ ] Define `ProcessType` enum: `ParentShell`, `ShellInternal`, `Subprocess`, `Script`
- [ ] `ProcessTable`: `LazyLock<RwLock<HashMap<(u64, u64), ProcessEntry>>>` — keyed by
  `(shell_id, pid)`. Per-shell PID counter: `HashMap<u64, AtomicU64>`.
- [ ] `ProcessTable::spawn(shell_id, ppid, argv, type_tag)` → PID; registers entry as
  `Running`
- [ ] `ProcessTable::complete(shell_id, pid, exit_code)` → marks `Zombie`
- [ ] `ProcessTable::reap(shell_id, pid)` → removes entry
- [ ] `ProcessTable::set_status(shell_id, pid, status)` → updates status
- [ ] Wire `dispatch_builtin` to register/complete process table entries around each
  `Process::run()` call
- [ ] Unit tests: spawn, complete, reap, status transitions

### Stream B: Core commands + VFS

- [ ] Add `LayeredVfs` to `clank-vfs/src/lib.rs`:
  - `mount_table: Vec<(PathBuf, Box<dyn VfsHandler>)>` checked prefix-first
  - Falls through to `RealFs` for unmounted paths
  - `VfsHandler` trait: `read_file`, `read_dir`, `stat`, `exists`
- [ ] Implement core command `Process` impls (each in its own file in `clank-shell/src/commands/`):
  - [ ] `LsProcess` — `ls [-la] [path]` via VFS
  - [ ] `CatProcess` — `cat [file...]` via VFS, stdin fallback
  - [ ] `GrepProcess` — `grep [-rnil] pattern [path...]` via VFS
  - [ ] `StatProcess` — `stat path` via VFS
  - [ ] `MkdirProcess` — `mkdir [-p] path` via VFS
  - [ ] `RmProcess` — `rm [-rf] path` via VFS (real paths only)
  - [ ] `CpProcess` — `cp src dst` via VFS
  - [ ] `MvProcess` — `mv src dst` via VFS
  - [ ] `TouchProcess` — `touch path` via VFS
  - [ ] `EnvProcess` — `env` via environment, redacting secret variables
- [ ] Register all above in `clank_builtins()` replacing stubs
- [ ] Unit tests for each command (VFS routes correctly, real FS fallthrough works)
- [ ] Crate integration tests via `ClankShell::run_line()` for each command
- [ ] Verify transcript capture: after `run_line("ls /tmp")`, transcript contains
  `Output` entry with listing

### Stream C: `/proc/` virtual namespace

- [ ] Define `ProcHandler` in `clank-vfs/src/proc_handler.rs`:
  - Reads from a `Arc<RwLock<ProcessTable>>` passed at construction
  - Serves `/proc/<pid>/cmdline` — space-separated argv
  - Serves `/proc/<pid>/status` — text format: `Pid: N\nPPid: N\nState: R\n...`
  - Serves `/proc/<pid>/environ` — NUL-separated `KEY=value` pairs; secret vars redacted
  - Serves `/proc/` directory — lists pids as subdirectory entries
  - Serves `/proc/clank/` and `/proc/clank/system-prompt` — assembled from
    `ManifestRegistry` + shell config
- [ ] Mount `ProcHandler` in `LayeredVfs` at `/proc`
- [ ] Integration tests: `cat /proc/<pid>/cmdline` returns correct argv
- [ ] Document known limitation: `cat < /proc/1/cmdline` (I/O redirect) does not work;
  `cat /proc/1/cmdline` (cat as builtin) does

### Stream D: `prompt-user` and `P` state

- [ ] Add `termimad` to `clank-shell` dependencies
- [ ] `PromptUserProcess` in `clank-shell/src/commands/prompt_user.rs`:
  - Parse flags: `--choices a,b,c`, `--confirm` (shorthand for `--choices yes,no`),
    `--secret`
  - Drain stdin (piped Markdown) and render to terminal using `termimad` — supports
    tables, emphasis, code blocks, and lists per the spec requirement
  - Display question text and choices/prompt
  - Set process status to `Paused` in process table
  - Block on `stdin.read_line()` until user responds
  - Validate response against `--choices` if given; re-prompt on invalid
  - On valid response: set status to `Zombie`, write response to stdout, exit 0
  - On Ctrl-C (`\x03` or `ErrorKind::Interrupted`): exit 130
  - `--secret`: response written to stdout but never to transcript; process registers
    a redaction rule for the variable receiving the value
- [ ] Register `PromptUserProcess` replacing stub
- [ ] Integration tests: `--confirm` yes/no, `--choices` validation, `--secret` not
  in transcript, exit 130 on Ctrl-C simulation
- [ ] System test: `echo "Approve?" | prompt-user --confirm` prompts correctly

### Stream E: Authorization model

- [ ] `ClankExportProcess` in `clank-shell/src/commands/export.rs`:
  - Implements `declaration_builtin: true` registration
  - Accepts `--secret` flag
  - Delegates all other logic to reimplemented Brush `export` behaviour
  - Records secret variable names in `SecretsRegistry` (global `LazyLock<RwLock<HashSet<String>>>` in `clank-shell`)
- [ ] `SecretsRegistry` consulted by `EnvProcess` and `/proc/<pid>/environ` handler
- [ ] Authorization enforcement in `run_line()`:
  - Look up command name in `GLOBAL_REGISTRY`
  - `Allow` → dispatch normally
  - `Confirm` → call `PromptUserProcess` inline, proceed only on yes, set process
    status to `Paused` during prompt
  - `SudoOnly` → check `SudoState` global; reject with exit 5 if not authorized
- [ ] `SudoState` global: `bool` indicating sudo-authorized for current invocation;
  set by `sudo` prefix handling; cleared after each command completes
- [ ] Update authorization policies in `ManifestRegistry::populate_defaults()`:
  - `rm` → `SudoOnly`
  - `mv`, `cp` → `Confirm` (when targeting `~` or system paths)
  - Keep `Allow` for read-only commands
  - For Phase 2: apply policies at the command level; per-argument policies deferred
- [ ] Integration tests: `Confirm` policy pauses and prompts; `SudoOnly` rejects
  without sudo; `sudo ls` (allow) works

### Stream F: Verification and quality gate

- [ ] Verify transcript capture for all newly implemented commands via `run_line()`
  integration tests
- [ ] Verify `ps aux` output format matches spec
- [ ] Update golden fixtures in `tests/fixtures/` for all changed command outputs
- [ ] `cargo test` — all tests pass
- [ ] `cargo clippy --all-targets -- -D warnings` — clean
- [ ] `cargo fmt --check` — clean

---

## Acceptance Tests

### Process table (A)

| Test | Assertion |
|---|---|
| `test_process_table_spawn_assigns_pid` | Each spawn gets a unique, increasing PID |
| `test_process_table_complete_marks_zombie` | Completed process transitions to Zombie |
| `test_process_table_reap_removes_entry` | Reaped process no longer in table |
| `test_process_table_status_transitions` | Running → Paused → Running → Zombie |

### Core commands (B)

| Test | Assertion |
|---|---|
| `test_ls_lists_real_directory` | `run_line("ls /tmp")` returns real entries |
| `test_cat_reads_real_file` | `run_line("cat /etc/hostname")` returns hostname |
| `test_grep_finds_pattern` | `run_line("grep root /etc/passwd")` returns matches |
| `test_transcript_captures_ls_output` | After `ls`, transcript has Output entry |
| `test_ls_proc_shows_pids` | `ls /proc/` lists active PIDs |

### `/proc/` namespace (C)

| Test | Assertion |
|---|---|
| `test_cat_proc_cmdline` | `cat /proc/<pid>/cmdline` returns correct argv |
| `test_cat_proc_status` | `cat /proc/<pid>/status` contains Pid, State fields |
| `test_cat_proc_system_prompt` | `cat /proc/clank/system-prompt` returns non-empty string |
| `test_proc_redirection_limitation` | Document that `< /proc/1/cmdline` fails gracefully |

### `prompt-user` (D)

| Test | Assertion |
|---|---|
| `test_prompt_user_confirm_yes` | `echo y \| prompt-user --confirm "ok?"` exits 0, stdout "y" |
| `test_prompt_user_confirm_no` | `echo n \| prompt-user --confirm "ok?"` exits 0, stdout "n" |
| `test_prompt_user_choices_invalid` | Invalid choice re-prompts |
| `test_prompt_user_secret_not_in_transcript` | `--secret` response absent from transcript |

### Authorization (E)

| Test | Assertion |
|---|---|
| `test_confirm_policy_prompts` | Command with `Confirm` policy pauses and calls prompt-user |
| `test_sudo_only_rejects_without_sudo` | `rm /tmp/x` (SudoOnly) fails with exit 5 |
| `test_export_secret_redacted_in_env` | `export --secret KEY=val; env` does not show val |
| `test_export_secret_not_in_proc_environ` | `cat /proc/<pid>/environ` omits secret vars |

---

## Known Limitations (documented, not blocking)

- `cat < /proc/1/cmdline` (I/O redirect to `/proc/`) does not work in Phase 2. Brush has
  no open hook. `cat /proc/1/cmdline` (cat as a registered builtin) works correctly.
  Upstream PR filed in parallel.
- Unregistered full-path invocations (`/usr/bin/vim`, `/bin/bash myscript.sh`) bypass
  transcript capture. Fully addressable only with the Brush open hook upstream.
- `Ctrl-Z` (SIGTSTP) not supported in Phase 2; native-only, Phase 5.
- Per-argument authorization policies (e.g. `confirm` only when `rm` targets `~`) deferred
  to Phase 5. Phase 2 enforces policies at the command level only.
