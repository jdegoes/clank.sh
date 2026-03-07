---
title: "Realized design: Pre-Phase 3 cleanup"
date: 2026-03-07
author: agent
plan: "dev-docs/plans/approved/pre-phase-3-cleanup.md"
---

# Realized design: Pre-Phase 3 cleanup

## What was built

Three items from the retrospective, plus comprehensive tutorial provider coverage.

---

## 1. `model default` implementation

**File:** `crates/clank-ask/src/model_process.rs`

`"default"` split from the `"remove" | "default" | "info"` stub arm. New `run_model_default(args)` function:

- No argument: loads config, prints `config.default_model` or `DEFAULT_MODEL` constant.
- With argument: loads config, sets `config.default_model = Some(model)`, saves atomically,
  prints `"Default model set to '<model>'.\n"`.

4 unit tests: prints builtin default when unconfigured, prints configured default, sets
model and persists it, does not clobber existing providers.

1 scenario fixture: `model/default_set_and_show.yaml` — adds anthropic key, sets default,
lists — confirms the new default appears in `model list` output and in `config_after`.

---

## 2. TUTORIAL.md — comprehensive provider coverage

**File:** `TUTORIAL.md`

Section 5 ("Configuring providers") expanded from covering only Anthropic and OpenRouter
to covering all four providers:

- **Option A — Anthropic** (cloud, pay-per-token): `model add anthropic --key <KEY>`,
  changing models via `model default`, link to Anthropic docs.
- **Option B — OpenRouter** (cloud, 300+ models): `model add openrouter --key <KEY>`,
  changing to non-Anthropic models, provider priority explanation, link to openrouter.ai/models.
- **Option C — Ollama** (local, free): install instructions, `model add ollama`, custom
  host configuration, troubleshooting messages for "not running" and "model not found".
- **Option D — OpenAI-compatible** (local, free): table of supported servers (llama.cpp,
  LM Studio, vLLM, LocalAI), concrete llama.cpp and LM Studio examples, API key option
  for servers that require auth.
- **Manual config file** example showing all four providers together.
- **Provider priority** explanation.

Section 4 updated to show accurate `model list` output format.

Section 12 "What does not work yet" updated:
- Removed rows for `ls`, `cat`, `grep`, `model add` — all implemented.
- Removed `/bin/ls` workaround note.
- Added explicit statement that all features documented in the tutorial are working.
- Remaining stubs: `ask repl`, `grease`, MCP, Golem, `model remove`, `model info`.

---

## 3. `prompt-user` Ctrl-C exit code tests

**File:** `crates/clank-shell/tests/prompt_user.rs` (new)

6 tests:

- `test_prompt_user_eof_exits_130` — `/dev/null` stdin → exit 130
- `test_prompt_user_confirm_eof_exits_130` — with `--confirm` flag
- `test_prompt_user_choices_eof_exits_130` — with `--choices` flag
- `test_prompt_user_eof_sets_dollar_question` — `$?` is 130 after EOF
- `test_shell_continues_after_prompt_user_eof` — shell loop does not exit on 130
- `test_prompt_user_only_exits_0_on_valid_response` — 0 is reserved for valid input

All tests use `</dev/null` shell redirection to trigger immediate EOF, which exercises
the same `read_response` code path as Ctrl-C at a real terminal.

## Test results

All 27 test suites pass. 89 tests in `clank-ask` (up from 85 — 4 new `model default`
tests). 6 new tests in `clank-shell/tests/prompt_user.rs`. `cargo clippy` and
`cargo fmt --check` clean.
