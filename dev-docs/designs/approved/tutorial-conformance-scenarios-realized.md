---
title: "Tutorial conformance scenario suite — realized design"
date: 2026-03-07
author: agent
---

# Realized Design: Tutorial conformance scenario suite

## What was built

### Tutorial corrections (T1)

**Section 9 (Authorization)** rewritten. The previous text described `Confirm` and `SudoOnly`
as applying to the user — incorrect. The corrected text explains:
- The user runs commands freely; policies apply to AI agent commands only
- `sudo` is a human gesture; `sudo ask` grants the agent broad authorization for one invocation
- Agents cannot use `sudo` on individual commands

**Section 8 (`/proc/`)** corrected. The `cat /proc/1/cmdline` example was removed — PID entries
are reaped synchronously before the next prompt. The section now shows `/proc/clank/` (always
present) and explains that per-process `/proc/` entries are visible during `P` (Paused) state.

**Section 3 (`context summarize`)** — provider restriction (Anthropic or OpenRouter required;
not Ollama/OpenAI-compat) added to the introductory paragraph.

### Scenario fixtures (T2)

18 fixtures in `crates/clank/tests/scenarios/tutorial/`, one per automatable tutorial snippet:

| Fixture | Tutorial section | What it covers |
|---|---|---|
| `section2_redirect_and_subshell.yaml` | §2 | Redirection + command substitution |
| `section3_context_show_with_content.yaml` | §3 | `context show` reflects prior commands |
| `section3_context_clear_empties_transcript.yaml` | §3 | `context clear` then `context show` empty |
| `section3_context_trim.yaml` | §3 | `context trim N` removes oldest entries |
| `section4_model_list_single_provider.yaml` | §4 | `model list` with one provider |
| `section5_model_add_anthropic.yaml` | §5 | `model add anthropic --key` |
| `section5_model_add_openrouter.yaml` | §5 | `model add openrouter --key` |
| `section5_model_add_ollama_custom_url.yaml` | §5 | `model add ollama --url` |
| `section5_model_default_set_and_show.yaml` | §5 | `model default` set and bare show |
| `section7_filesystem_workflow.yaml` | §7 | `mkdir` + `cd` + file creation (cd fix regression) |
| `section8_ps_format.yaml` | §8 | `ps` column header format |
| `section8_proc_clank.yaml` | §8 | `/proc/clank/system-prompt` always readable |
| `section9_user_rm_no_auth_required.yaml` | §9 | `rm` runs freely for user (not exit 5) |
| `section9_user_sudo_rm_works.yaml` | §9 | `sudo rm` runs for user |
| `section10_export_secret_masked_in_env.yaml` | §10 | `export --secret` + `env` exits cleanly |
| `section10_export_secret_usable_in_shell.yaml` | §10 | Secret variable accessible in shell |
| `section11_prompt_user_basic.yaml` | §11 | `prompt-user` captures response |
| `section11_prompt_user_choices.yaml` | §11 | `prompt-user --choices` with valid input |

Excluded (non-deterministic): `ask "..."` calls, `context summarize`, `--json` model responses.

### `VfsError` display fixed (discovered during this work)

`VfsError::NotFound` previously displayed `not found: /path/to/file`, repeating the path
that the calling command already included in its error message. Changed to
`No such file or directory` (POSIX convention). `VfsError::PermissionDenied` similarly
changed from `permission denied: /path` to `permission denied`.

## Key decisions

- Fixtures use sandbox-relative paths (via `{cwd}` token or bare relative paths) rather than
  `/tmp/` to avoid macOS symlink expansion (`/tmp` → `/private/tmp`) causing stderr mismatches
- `env` output is machine-specific — `section10_export_secret_masked_in_env.yaml` asserts
  only `stderr: ''` (no errors) rather than exact stdout; the masking contract is covered by
  Level 2 integration tests in `clank-shell/tests/`
- `prompt-user` prompt text goes to stderr; response to stdout — both are asserted

## Methodology note

The original plan classified the `/proc/<pid>/cmdline` snippet as "partially automatable —
PIDs non-deterministic" and skipped it. This was wrong: the fact that the path is readable
at all is deterministic and testable. The fixture attempt would have immediately revealed the
bug (PID already reaped). Going forward: attempt a fixture for every deterministic tutorial
snippet; let failures surface bugs; only skip genuinely non-deterministic output.
