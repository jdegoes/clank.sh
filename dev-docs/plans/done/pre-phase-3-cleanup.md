---
title: "Plan: Pre-Phase 3 cleanup"
date: 2026-03-07
author: agent
issue: "dev-docs/issues/open/pre-phase-3-cleanup.md"
---

# Plan: Pre-Phase 3 cleanup

## Originating Issue

`dev-docs/issues/open/pre-phase-3-cleanup.md` — three small items from the retrospective
that should be resolved before Phase 3 planning begins.

---

## Item 1 — `model default <model>` implementation

### What it does

`model default <model>` writes `default_model = "<model>"` to `ask.toml` and prints a
confirmation. `model default` with no argument prints the current default and exits 0.

### Implementation

In `crates/clank-ask/src/model_process.rs`, split `"default"` out of the
`"remove" | "default" | "info"` stub arm:

```rust
"default" => run_model_default(&argv[2..]),
```

New function:

```rust
fn run_model_default(args: &[String]) -> (String, String, i32) {
    match args.first().map(String::as_str) {
        None | Some("") => {
            // No argument — print current default.
            let config = match AskConfig::load_or_default() {
                Ok(c) => c,
                Err(e) => return (String::new(), format!("clank: model default: {e}\n"), 1),
            };
            let model = config
                .default_model
                .as_deref()
                .unwrap_or(crate::config::DEFAULT_MODEL);
            (format!("{model}\n"), String::new(), 0)
        }
        Some(model) => {
            let mut config = match AskConfig::load_or_default() {
                Ok(c) => c,
                Err(e) => return (String::new(), format!("clank: model default: {e}\n"), 1),
            };
            config.default_model = Some(model.to_string());
            if let Err(e) = config.save() {
                return (String::new(), format!("clank: model default: {e}\n"), 1);
            }
            (format!("Default model set to '{model}'.\n"), String::new(), 0)
        }
    }
}
```

### Tests

Level 1 unit tests in `model_process.rs` (within `with_temp_config`):

```
test_model_default_no_arg_prints_builtin_default
  - No config file → prints DEFAULT_MODEL constant.

test_model_default_no_arg_prints_configured_default
  - Config has default_model set → prints it.

test_model_default_sets_model
  - `model default anthropic/claude-haiku-3-5` writes default_model to config.
  - Subsequent `model default` (no arg) prints the new value.

test_model_default_does_not_clobber_providers
  - Config has existing providers. After `model default <model>`, providers unchanged.
```

Level 3 scenario fixture: `tests/scenarios/model/default_set_and_show.yaml`:

```yaml
desc: "model default sets the default model and model list reflects it"
stdin: |
  model add anthropic --key sk-test
  model default anthropic/claude-haiku-3-5
  model list
stdout: |
  $ Provider 'anthropic' configured.
  $ Default model set to 'anthropic/claude-haiku-3-5'.
  $ Default model: anthropic/claude-haiku-3-5

  Providers:
    anthropic: api_key configured
  $ 
config_after:
  default_model: "anthropic/claude-haiku-3-5"
```

---

## Item 2 — TUTORIAL.md update

### Changes required

**Section 12 "What does not work yet"** — remove or update three stale rows:

| Current (stale) | Corrected |
|---|---|
| `ls`, `cat`, `grep` and other core Unix commands — Phase 3 stubs | Remove this row entirely (they work now) |
| `model add`, `model default` — Planned (local provider support in progress) | Update: `model add` implemented; `model default` now implemented; remove row |
| The note about `/bin/ls` workaround | Remove entirely |

**Other changes:**

- Add `model default` to section 5 ("Configuring an API key") under the `model` commands
  overview, showing that the default model can be changed:

  ```
  $ model default anthropic/claude-haiku-3-5
  Default model set to 'anthropic/claude-haiku-3-5'.
  ```

- Update section 4 ("Checking your configuration") — the `model list` output shown is
  correct but the table in section 12 contradicts it. Removing the stale rows is sufficient.

- The "What does not work yet" table after the update:

  | Feature | When |
  |---|---|
  | `ask repl` (persistent conversation session) | Phase 4 |
  | MCP tool integration | Phase 3 |
  | Package installation (`grease`) | Phase 3 |
  | Golem cloud deployment | Phase 4 |
  | `model remove`, `model info` | Planned |

No other sections of TUTORIAL.md need changes — the rest accurately describes current behaviour.

---

## Item 3 — `prompt-user` Ctrl-C exit code test

### What the research found

The implementation is correct: `read_response` returns `Err(130)` on EOF or error from
`read_line` (non-secret path) and from `rpassword::read_password()` (secret path).
`PromptUserProcess::run` maps `Err(130)` → `ProcessResult::failure(130)`. Exit 130
propagates correctly through `dispatch_builtin` to Brush as `Custom(130)`.

The only gap is that there is no test asserting this contract.

### Test to add

Because `prompt-user` reads from the real terminal stdin (not from `ProcessIo.stdin`),
testing Ctrl-C requires simulating EOF on real stdin. The clean way to do this in a
test is to run `read_response` directly with a mock stdin that immediately returns
`Ok(0)` (EOF). However, `read_response` is a private function.

The practical approach is a **Level 2 test via `run_line`** using a real shell that has
`prompt-user` registered. We can simulate EOF by passing stdin that immediately closes.
`ClankShell::run_line("prompt-user 'question'")` with the real stdin already at EOF (in
a non-interactive test environment where stdin is not a terminal) will trigger the
`Ok(0)` EOF path.

Test in `crates/clank-shell/tests/context.rs` or a new `tests/prompt_user.rs`:

```
test_prompt_user_eof_exits_130
  - Create a shell. Run `prompt-user "test question"`.
  - In a test environment, real stdin is not a terminal and read_line will return
    Ok(0) (EOF) immediately.
  - Assert exit code is 130.

test_prompt_user_success_exits_0
  - This already works in production but add an explicit test asserting exit 0 and
    that stdout contains the response. Uses a mock approach or the piped-stdin path.
```

Note: If the test environment's stdin is not immediately EOF (e.g. it blocks), we need
to close stdin before the test. This can be done by closing fd 0 or by running the
command with stdin redirected from `/dev/null` at the shell level:
`shell.run_line("prompt-user 'question' </dev/null")`.

The `/dev/null` redirect approach is the most robust — Brush handles `</dev/null` as
an explicit stdin redirect, so `read_line` on the real stdin (which `prompt-user` reads
from) will get EOF immediately.

---

## Changed files

| File | Change |
|---|---|
| `crates/clank-ask/src/model_process.rs` | Item 1: implement `run_model_default`; add unit tests |
| `crates/clank/tests/scenarios/model/default_set_and_show.yaml` | Item 1: new scenario fixture |
| `TUTORIAL.md` | Item 2: update stale rows in section 12; add `model default` example |
| `crates/clank-shell/tests/prompt_user.rs` (new) | Item 3: `prompt-user` Ctrl-C and success tests |

---

## Acceptance criteria

1. `model default anthropic/claude-haiku-3-5` writes the model to config and prints confirmation.
2. `model default` (no arg) prints the currently configured default.
3. `model list` reflects the new default after `model default` is run.
4. TUTORIAL.md section 12 no longer references `ls`, `cat`, `grep`, or `model add` as unimplemented.
5. A test asserts `prompt-user 'question' </dev/null` exits 130.
6. `cargo test --workspace`, `cargo clippy -- -D warnings`, `cargo fmt --check` all pass.

---

## Tasks

- [ ] **P1** Implement `run_model_default()` in `model_process.rs`; split `"default"` from the stub arm; add 4 unit tests
- [ ] **P2** Add scenario fixture `default_set_and_show.yaml`; run `CLANK_UPDATE=1` to confirm expected output
- [ ] **P3** Update TUTORIAL.md: remove stale rows from section 12; add `model default` example; remove `/bin/ls` workaround note
- [ ] **P4** Add `crates/clank-shell/tests/prompt_user.rs` with `test_prompt_user_eof_exits_130` and `test_prompt_user_success_exits_0`
- [ ] **QG** `cargo test --workspace`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check` all pass; acceptance criteria 1–5 verified
