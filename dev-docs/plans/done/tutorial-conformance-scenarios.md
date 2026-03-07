---
title: "Tutorial conformance scenario suite"
date: 2026-03-07
author: agent
issue: []
research: []
designs: []
---

# Plan: Tutorial conformance scenario suite

## Context

The tutorial (`TUTORIAL.md`) is the primary user-facing contract for clank.sh. A peer reviewer
or new user will work through it sequentially. When a tutorial snippet fails or produces wrong
output, it damages confidence in the whole project.

Scenario fixtures at Level 3 drive the compiled `clank` binary with exact stdin command
sequences and assert on exact stdout/stderr output and resulting config state. They are the
right tool for asserting tutorial snippets because they test the full binary-level contract
without requiring network access or non-deterministic model output.

This plan adds a `tutorial/` scenario directory whose fixtures map directly to tutorial
sections. Each fixture is named after the section and snippet it covers. If a tutorial
snippet ever breaks, the corresponding fixture fails and identifies exactly which section is
affected.

---

## What is automatable vs. not

The following tutorial snippets require live model API calls and are **excluded**:
- Section 6: `ask "..."` (all variants — non-deterministic output)
- Section 3: `context summarize` (requires live model)
- Section 7: The `ask "..."` call in the worked example (non-deterministic)
- Section 6: `--json` model responses (non-deterministic)

Everything else is automatable. The fixtures below cover 11 of the 14 tutorial sections.

---

## Tutorial section analysis and fixture mapping

### Section 2 — Basic shell usage (already covered)

`echo`, `export && echo`, `false || echo`, `false ; echo`, multi-line `if/fi` are all
covered by existing `shell_basics/` fixtures. No new fixtures needed.

The redirection snippet `echo "..." > /tmp/test.txt && echo $(< /tmp/test.txt)` is not
covered — add it.

### Section 3 — Session transcript

`context show` (empty) and `context clear` are covered by existing `ask/` fixtures.
Not covered:
- `context show` with actual transcript content (commands + output visible)
- `context clear` followed by `context show` showing empty result
- `context trim <n>` removes oldest entries

### Section 4 — Checking configuration (`model list`)

`model list` with no config is covered by `ask/model_list_no_config.yaml`.
Not covered:
- `model list` with a single provider configured (Anthropic example from tutorial)
- `model list` with multiple providers (all four from tutorial)

### Section 5 — Configuring providers

`model add ollama`, `model add openai-compat --url`, `model add openai-compat --url --key`
are covered by existing `model/` fixtures.
Not covered:
- `model add anthropic --key <KEY>` → exact stdout + config_after
- `model add openrouter --key <KEY>` → exact stdout + config_after
- `model add ollama --url <custom>` → stdout + config_after with custom url
- `model default <model>` → stdout and config_after
- `model default` (no args) → shows current default
- The combined `model default` + `model list` flow from section 5

### Section 7 — Complete worked example (filesystem operations)

The `mkdir`/`cd`/`cat`/`chmod`/`./hello.sh` sequence (excluding the `ask` call) is not
covered anywhere. This is the most important tutorial fixture because it exercises the
`cd`-after-`mkdir` fix and the full VFS + OS-fallthrough pipeline.

### Section 8 — Process table (`ps`)

`ps` output format is not covered by any scenario fixture. PIDs are non-deterministic so
we cannot assert exact PID values, but we can assert the column header format and that the
output contains `STAT` and `COMMAND`.

### Section 9 — Authorization

The tutorial's authorization section (section 9) describes `Confirm` and `SudoOnly` as
user-facing policies — but this is now incorrect. Per the Phase 1 fix, these policies are
not enforced for user-typed commands. The tutorial must be corrected before fixtures are
written.

The corrected tutorial should explain:
- In the interactive shell, commands run without confirmation prompts — you are the user.
- `Confirm` and `SudoOnly` policies apply to commands issued by the AI agent (Phase 3).
- `sudo rm` still works for human use as a conventional prefix.

After correcting the tutorial, add fixtures that verify the corrected behaviour:
- `rm /tmp/nonexistent` exits 1 (rm's error) not 5 (auth denied) — user context
- `sudo rm /tmp/nonexistent` exits 1 — sudo prefix works, still rm's error

### Section 10 — `export --secret`

Not covered. Add fixture:
- `export --secret KEY=value` followed by `env` shows `KEY=***`
- The variable is accessible in the shell (`echo $KEY` outputs the value)

### Section 11 — `prompt-user`

`prompt-user` with piped stdin is the only automatable form. Interactive prompts reading
from real stdin cannot be reliably asserted in fixtures (they would block or require TTY
emulation). The fixture must pipe the response via stdin.

Not covered:
- `prompt-user "question"` with a response piped via stdin → stdout shows question + response
- `prompt-user --choices yes,no "question"` with valid choice piped → stdout + exit 0
- `prompt-user --choices yes,no "question"` with invalid choice → loops (not automatable)
- `prompt-user --confirm "question"` with "yes" piped → exit 0, stdout "yes"

---

## Tasks

- [ ] **T1 — Correct TUTORIAL.md section 9 (Authorization)**

  Rewrite section 9 to accurately reflect Phase 1 behaviour:
  - The user is never prompted for confirmation or denied for `rm` — these policies apply
    to AI agent commands only (Phase 3).
  - `sudo rm` works as a conventional prefix (strips `sudo`, executes `rm`).
  - Remove the `Confirm` example showing `curl` prompting the user.
  - Replace with a clear explanation of the user-vs-agent authorization model.

- [ ] **T2 — Add `tutorial/` scenario directory and the following fixtures:**

  **`tutorial/section2_redirect_and_subshell.yaml`**
  ```
  stdin: echo "written to file" > /tmp/clank-tut-test.txt && echo $(< /tmp/clank-tut-test.txt)
  stdout: assert contains "written to file"
  ```
  Covers the redirection + command substitution snippet from section 2.

  **`tutorial/section3_context_show_with_content.yaml`**
  ```
  stdin: echo "some work"\necho "more work"\ncontext show
  stdout: assert contains "$ echo \"some work\"" and "some work" and "$ echo \"more work\""
  ```
  Verifies that `context show` reflects prior commands and their output.

  **`tutorial/section3_context_clear_empties_transcript.yaml`**
  ```
  stdin: echo "some work"\ncontext clear\ncontext show
  stdout: assert "context show" after clear shows empty transcript marker
  ```

  **`tutorial/section3_context_trim.yaml`**
  ```
  stdin: echo "one"\necho "two"\necho "three"\ncontext trim 2\ncontext show
  stdout: assert does NOT contain "$ echo \"one\"" (oldest two trimmed)
           assert DOES contain "$ echo \"three\""
  ```
  Verifies trim removes the oldest N entries.

  **`tutorial/section4_model_list_single_provider.yaml`**
  ```
  config: providers.anthropic.api_key = "sk-ant-test"
  stdin: model list
  stdout: exact match — default model line + "anthropic: api_key configured"
  ```

  **`tutorial/section5_model_add_anthropic.yaml`**
  ```
  stdin: model add anthropic --key sk-ant-test-key
  stdout: "Provider 'anthropic' configured.\n"
  config_after: providers.anthropic.api_key = "sk-ant-test-key"
  ```

  **`tutorial/section5_model_add_openrouter.yaml`**
  ```
  stdin: model add openrouter --key sk-or-test-key
  stdout: "Provider 'openrouter' configured.\n"
  config_after: providers.openrouter.api_key = "sk-or-test-key"
  ```

  **`tutorial/section5_model_add_ollama_custom_url.yaml`**
  ```
  stdin: model add ollama --url http://192.168.1.10:11434
  stdout: "Provider 'ollama' configured.\n"
  config_after: providers.ollama.base_url = "http://192.168.1.10:11434"
  ```

  **`tutorial/section5_model_default_set_and_show.yaml`**
  ```
  config: providers.anthropic.api_key = "sk-ant-test"
  stdin: model default anthropic/claude-haiku-3-5\nmodel default
  stdout: assert contains "Default model set to 'anthropic/claude-haiku-3-5'."
          assert contains "anthropic/claude-haiku-3-5" (from bare `model default`)
  ```

  **`tutorial/section7_filesystem_workflow.yaml`**
  ```
  stdin: cd /tmp
         mkdir clank-tut-demo-<uuid>
         cd clank-tut-demo-<uuid>
         echo "#!/bin/bash" > hello.sh
         echo 'echo "Hello, $1!"' >> hello.sh
         cat hello.sh
  stdout: assert contains "#!/bin/bash" and 'echo "Hello, $1!"'
  ```
  Verifies the `cd` + `mkdir` + `cd` + file-creation workflow from section 7.
  (The `./hello.sh` execution and `ask` call are excluded — require OS execution permissions
  and live model respectively.)

  **`tutorial/section8_ps_format.yaml`**
  ```
  stdin: ps
  stdout: assert contains "PID" and "STAT" and "COMMAND"
  ```
  Verifies `ps` produces the correct column header. PID values are not asserted.

  **`tutorial/section9_user_rm_no_auth_required.yaml`**
  (After correcting tutorial section 9)
  ```
  stdin: rm /tmp/clank-tut-nonexistent-file
  stderr: assert contains "rm:" (rm's own error message)
  exit_code: 1 (rm's own error, not 5 for auth)
  ```

  **`tutorial/section9_user_sudo_rm_works.yaml`**
  ```
  stdin: sudo rm /tmp/clank-tut-nonexistent-file
  stderr: assert contains "rm:" not "requires sudo authorization"
  exit_code: 1
  ```

  **`tutorial/section10_export_secret_masked_in_env.yaml`**
  ```
  stdin: export --secret GITHUB_TOKEN=ghp_test\nenv
  stdout: assert contains "GITHUB_TOKEN=***"
  ```
  Verifies the secret masking behaviour from section 10.

  **`tutorial/section10_export_secret_usable_in_shell.yaml`**
  ```
  stdin: export --secret GITHUB_TOKEN=ghp_test\necho $GITHUB_TOKEN
  stdout: assert contains "ghp_test"
  ```
  Verifies the variable is still usable despite being secret.

  **`tutorial/section11_prompt_user_basic.yaml`**
  ```
  stdin: prompt-user "Which branch?" <<< "main"
  stdout: assert contains "Which branch?" and "main"
  ```
  Note: `prompt-user` reads from stdin. The `<<<` here feed input through the shell's
  heredoc into prompt-user's stdin. Verify this works in the scenario harness — if not,
  pipe via echo: `echo "main" | prompt-user "Which branch?"`.

  **`tutorial/section11_prompt_user_choices.yaml`**
  ```
  stdin: echo "yes" | prompt-user --choices yes,no "Are you sure?"
  stdout: assert contains "Are you sure?" and "yes"
  exit_code: 0
  ```

---

## Acceptance criteria

1. All new scenario fixtures pass with `cargo test --test scenario`.
2. TUTORIAL.md section 9 correctly describes user-vs-agent authorization.
3. No existing scenario fixtures are broken.
4. Each fixture file includes a `desc:` that names the tutorial section and snippet it covers.
5. Fixture names map clearly to tutorial sections (`section<N>_<description>.yaml`).

---

## Implementation notes

- Use `CLANK_UPDATE=1 cargo test --test scenario` to generate initial `stdout:` expected
  values after verifying the output is correct by inspection.
- The `tutorial/section7_filesystem_workflow.yaml` fixture requires the `cd`-after-`mkdir`
  fix (now complete) to pass — it is the regression fixture for that bug.
- `context trim` assertions use substring matching (`stdout_contains`) not exact match, as
  the exact prompt count depends on how many entries were created. Check if the scenario
  harness supports substring assertions; if not, use exact match with careful stdin design.
- The `prompt-user` fixtures must be verified against actual harness behaviour — `prompt-user`
  reads stdin, which in the scenario harness is the command sequence itself. We may need
  to use process substitution or a subshell to pipe input cleanly. Investigate at
  implementation time.
- T1 (tutorial correction) must be done before T2 so the authorization fixtures assert the
  correct behaviour.
- Clean up any `/tmp/clank-tut-*` directories created during fixture runs. Use unique names
  (e.g. include a constant suffix) and add `files_after:` assertions or explicit cleanup
  commands in the fixture stdin.
