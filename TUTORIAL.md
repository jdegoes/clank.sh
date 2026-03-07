# clank.sh Tutorial

This tutorial walks you through building and using clank.sh. The shell itself works without
an API key — only the AI features require one. You can use any of:

- **Anthropic** directly (pay-per-token, cloud)
- **OpenRouter** (access to hundreds of models via a single key)
- **Ollama** (local, free, no API key)
- **Any OpenAI-compatible server** — llama.cpp, LM Studio, vLLM, LocalAI (local, free)

---

## 1. Build

```sh
git clone <repo>
cd clank.sh
cargo build
```

The binary lands at `target/debug/clank`. You can run it directly or add it to your `$PATH`.

```sh
./target/debug/clank
```

You will see a `$ ` prompt. Press `Ctrl-D` or type `exit` to quit.

---

## 2. Basic shell usage

clank.sh is a bash-compatible shell. Ordinary commands work as you would expect.

```
$ echo "hello world"
hello world
$ pwd
/home/you/clank.sh
$ export GREETING=hello && echo $GREETING
hello
```

Pipes, redirections, and multi-command sequences all work:

```
$ false || echo "fallback"
fallback
$ false ; echo "always runs"
always runs
$ echo "written to file" > /tmp/test.txt && echo $(< /tmp/test.txt)
written to file
```

Multi-line constructs — `if/fi`, `while/done`, `for/done` — are supported. The shell shows
`> ` while waiting for the construct to be completed:

```
$ if true; then
> echo "inside if"
> fi
inside if
```

---

## 3. The session transcript

Every command you type and every output it produces is recorded in a sliding-window transcript.
This is what the AI model reads when you use `ask`. The `context` builtin manages it.

### Viewing the transcript

```
$ echo "some work"
some work
$ echo "more work"
more work
$ context show
$ echo "some work"
some work
$ echo "more work"
more work
$ context show
```

You will see your full session history — commands and their outputs — formatted with `$ `
prefixes on commands.

### Clearing the transcript

```
$ context clear
```

After this, `context show` will display nothing — the AI will start the next `ask` with
a blank slate.

### Trimming old entries

```
$ context trim 5
```

This drops the five oldest entries from the transcript, freeing space while keeping recent
history. Useful before a long task if the session has a lot of unrelated history.

### Summarising the transcript

With an Anthropic or OpenRouter key configured (see section 5), `context summarize` calls
the model and prints a condensed summary of your session so far. Note that this subcommand
requires a cloud provider key — it does not work with Ollama or an OpenAI-compatible local
server:

```
$ context summarize
The session explored the project directory structure, ran the test suite, and identified
two failing tests related to HTTP error handling.
```

This is the building block for manual compaction:

```sh
SUMMARY=$(context summarize) && context clear && echo "$SUMMARY"
```

After running this, the transcript contains only the summary — the model retains orientation
without burning through its context budget on old history.

---

## 4. Checking your configuration

```
$ model list
```

If you have not yet configured any provider, you will see:

```
No providers configured.
Run: model add anthropic --key <KEY>
```

If you have configured one or more providers (example with several):

```
Default model: anthropic/claude-sonnet-4-5

Providers:
  anthropic:     api_key configured
  ollama:        base_url=http://localhost:11434
  openai-compat: base_url=http://localhost:8080
  openrouter:    api_key configured
```

With just Anthropic configured:

```
Default model: anthropic/claude-sonnet-4-5

Providers:
  anthropic: api_key configured
```

---

## 5. Configuring providers

clank supports four model providers. Use whichever works for your setup — or combine
several. Configuration is stored in `ask.toml` and managed with `model add`.

**Config file location:**
- Linux: `~/.config/ask/ask.toml`
- macOS: `~/Library/Application Support/ask/ask.toml`

Override with `CLANK_CONFIG` for project-local or CI setups:

```sh
CLANK_CONFIG=./ask.toml clank         # config in current directory
export CLANK_CONFIG=~/work/ask.toml   # exported for the whole session
```

---

### Option A — Anthropic (cloud, pay-per-token)

Get an API key from [console.anthropic.com](https://console.anthropic.com).

```
$ model add anthropic --key sk-ant-your-key-here
Provider 'anthropic' configured.
```

This writes to `ask.toml`. The default model is `anthropic/claude-sonnet-4-5`.

**Using a different Anthropic model:**

```
$ model default anthropic/claude-haiku-3-5
Default model set to 'anthropic/claude-haiku-3-5'.
```

Available models: `claude-opus-4`, `claude-sonnet-4-5`, `claude-haiku-3-5`, and others.
Check the [Anthropic docs](https://docs.anthropic.com/en/docs/about-claude/models) for the
full list.

---

### Option B — OpenRouter (cloud, 300+ models, one key)

Get an API key from [openrouter.ai](https://openrouter.ai). OpenRouter routes to
Anthropic, OpenAI, Google, Mistral, Meta, and dozens of other providers through a single
key — useful if you want access to multiple model families without managing separate keys.

```
$ model add openrouter --key sk-or-your-key-here
Provider 'openrouter' configured.
```

To use a non-Anthropic model as your default:

```
$ model default openai/gpt-4o
Default model set to 'openai/gpt-4o'.
$ model default google/gemini-2.0-flash-001
Default model set to 'google/gemini-2.0-flash-001'.
$ model default meta-llama/llama-3.3-70b-instruct
Default model set to 'meta-llama/llama-3.3-70b-instruct'.
```

Override per-invocation with `--model`:

```
$ ask --model openai/gpt-4o "quick question: explain monads"
```

The full list of available models is at [openrouter.ai/models](https://openrouter.ai/models).

**Provider priority:** If you have both an `anthropic` key and an `openrouter` key, the
direct Anthropic key is used for `anthropic/...` models; OpenRouter is the fallback for
everything else.

---

### Option C — Ollama (local, free, no API key)

[Ollama](https://ollama.com) runs models locally on your machine. Install it, pull a model,
and clank connects with no API key required.

```sh
# Install Ollama (once)
curl -fsSL https://ollama.com/install.sh | sh

# Pull a model
ollama pull llama3.2
```

Then configure clank:

```
$ model add ollama
Provider 'ollama' configured.
```

This registers Ollama at `http://localhost:11434` (the default). Set the default model:

```
$ model default ollama/llama3.2
Default model set to 'ollama/llama3.2'.
```

**Custom Ollama host** (e.g. Ollama on a remote machine or non-default port):

```
$ model add ollama --url http://192.168.1.10:11434
Provider 'ollama' configured.
```

**Other models via Ollama** — pull any model Ollama supports:

```sh
ollama pull mistral
ollama pull phi4
ollama pull gemma3:12b
```

Then use it:

```
$ ask --model ollama/mistral "explain this error"
$ model default ollama/phi4
```

To see which models you have pulled: `ollama list`.

**Troubleshooting:**
- `Ollama is not running at http://localhost:11434. Start it with: ollama serve` — Ollama
  is not running. Run `ollama serve` (or check that the Ollama desktop app is open).
- `Model 'llama3.2' not found. Pull it with: ollama pull llama3.2` — the model has not
  been pulled yet.

---

### Option D — OpenAI-compatible local server (llama.cpp, LM Studio, vLLM, LocalAI)

Any server that speaks the OpenAI `/v1/chat/completions` API works. The most common:

| Server | Default URL | Notes |
|---|---|---|
| llama.cpp (`llama-server`) | `http://localhost:8080` | Single model, no auth |
| LM Studio | `http://localhost:1234` | GUI app, select model in UI |
| vLLM | `http://localhost:8000` | Multi-model, may require an API key |
| LocalAI | `http://localhost:8080` | Multi-model |
| Ollama (OpenAI compat) | `http://localhost:11434` | Use Option C instead |

**Setting up llama.cpp as an example:**

```sh
# Build llama.cpp, download a model, then start the server:
./llama-server -m models/phi-4.gguf --port 8080
```

Configure clank to use it:

```
$ model add openai-compat --url http://localhost:8080
Provider 'openai-compat' configured.
$ model default openai-compat/phi-4
Default model set to 'openai-compat/phi-4'.
```

The model name after `openai-compat/` is passed to the server as the `model` field.
Single-model servers (llama-server, LM Studio) typically ignore it; multi-model servers
(vLLM, LocalAI) use it to select which model to run.

**With authentication** (some servers require a key):

```
$ model add openai-compat --url http://localhost:8080 --key my-local-key
Provider 'openai-compat' configured.
```

**LM Studio specifically** — enable the local server in the LM Studio UI, load a model,
then:

```
$ model add openai-compat --url http://localhost:1234
Provider 'openai-compat' configured.
```

---

### Viewing and changing the default model

```
$ model default                              # show current default
anthropic/claude-sonnet-4-5
$ model default ollama/llama3.2             # change default
Default model set to 'ollama/llama3.2'.
$ model list                                 # full provider status
Default model: ollama/llama3.2

Providers:
  anthropic: api_key configured
  ollama:    base_url=http://localhost:11434
```

Override the default for a single `ask` invocation with `--model`:

```
$ ask --model anthropic/claude-opus-4 "explain this carefully"
```

### Manual config file

All `model add` and `model default` commands write to `ask.toml`. You can also edit it
directly:

```toml
default_model = "ollama/llama3.2"

[providers.anthropic]
api_key = "sk-ant-your-key-here"

[providers.openrouter]
api_key = "sk-or-your-key-here"

[providers.ollama]
base_url = "http://localhost:11434"

[providers.openai-compat]
base_url = "http://localhost:8080"
# api_key = "optional-key"
```

You can have multiple providers configured simultaneously. The provider used for a given
`ask` call is determined by the model name prefix: `anthropic/` → Anthropic, `openai/` →
OpenRouter, `ollama/` → Ollama, `openai-compat/` → OpenAI-compat server. Unrecognised
prefixes fall through to OpenRouter if an OpenRouter key is configured.

---

## 6. Using `ask`

With a key configured, `ask` invokes the model with your session transcript as context.

### Basic usage

```
$ ls -la
total 48
drwxr-xr-x  8 you  staff   256 Mar  6 14:00 .
...
$ ask "what does that output tell me about this directory?"
The directory contains 8 entries. The `.` and `..` entries are the current and parent
directories. You have read, write, and execute permissions on all files...
```

The model saw the `ls -la` output because it was captured in the transcript.

### Using context flags

**Start fresh** — ignore the current session, useful in scripts:

```
$ ask --fresh "what is the capital of France?"
Paris.
```

**Explicitly inherit** — the default behaviour, but stated clearly for readability in scripts:

```
$ ask --inherit "based on what we've done so far, what should I do next?"
```

**Choose a model** — override the default for a single call:

```
$ ask --model anthropic/claude-haiku-3-5 "quick question: what does 2 + 2 equal?"
4
```

### Expecting JSON output

Use `--json` when you need to pipe the result to another tool. If the model does not return
valid JSON, `ask` exits with code 6 and emits the raw response on stderr:

```
$ ask --json "list three Unix commands as a JSON array of strings"
["ls", "grep", "awk"]
$ ask --json "list three Unix commands as a JSON array" | jq '.[0]'
"ls"
```

### Piping supplementary input

Content piped into `ask` arrives as supplementary context alongside the transcript.
The transcript comes first; piped input is appended as an additional channel:

```
$ cat error.log | ask "what is causing this error?"
```

Or from a command:

```
$ git diff HEAD | ask "summarise these changes in one sentence"
```

This works for any content you want the model to reason about without it being part of
the permanent session transcript.

---

## 7. A complete worked example

This session demonstrates the transcript in action across multiple commands:

```
$ cd /tmp
$ mkdir demo && cd demo
$ cat > hello.sh << 'EOF'
#!/bin/bash
echo "Hello, $1!"
EOF
$ chmod +x hello.sh
$ ./hello.sh World
Hello, World!
$ ./hello.sh
Hello, !
$ ask "the script works when I pass an argument but not without one — how should I fix it?"
The issue is that `$1` is empty when no argument is provided. You can fix this by giving
the parameter a default value:

    echo "Hello, ${1:-World}!"

This uses bash parameter expansion: if `$1` is unset or empty, it falls back to `"World"`.
```

The model knew what the script contained, what command was run, and what output was produced
— all from the transcript, without you repeating any of it.

---

## 8. The process table and `ps`

Every command you run is tracked in a synthetic process table. `ps` reads directly from it.

```
$ ps
  PID STAT COMMAND
```

After running a few commands the table will be mostly empty between prompts (processes are
reaped synchronously). The table is most visible when `prompt-user` is active — that process
will appear in `P` (Paused) state while waiting for your input.

`ps aux` and `ps -ef` produce the standard extended column formats:

```
$ ps aux
USER       PID  %CPU %MEM   VSZ   RSS TTY  STAT START TIME COMMAND
you        1    -    -    -    -    -    R    -    -    context show
```

Non-meaningful columns (CPU, memory, start time) display `-` — clank tracks process
identity and state, not kernel resource metrics.

The virtual `/proc/` filesystem mirrors the process table. Each process gets its own
directory **while it is running or paused**. In the interactive REPL, processes complete
and are reaped before the next prompt appears, so `/proc/<pid>/` entries are gone by the
time you type the next command.

`/proc/<pid>/` is most useful in two situations:

**1. During a `prompt-user` pause.** When the AI runs `prompt-user`, that process enters
`P` (Paused) state and stays in the table until you respond. A second terminal or a
background job could read its `/proc/` entry while it waits.

**2. From a script, using `$!` or `$$`.** The shell itself has a persistent entry at
`/proc/clank/`:

```
$ ls /proc/clank
system-prompt
$ cat /proc/clank/system-prompt
(system prompt not configured)
```

`/proc/clank/system-prompt` shows the system prompt that will be sent to the model on the
next `ask` invocation — useful for debugging what context the model will see.

---

## 9. Authorization — `sudo` and command policies

Every command in clank has an authorization policy in its manifest. There are three levels:

- **Allow** — runs immediately (most commands).
- **Confirm** — pauses for user confirmation before proceeding.
- **SudoOnly** — requires a `sudo` prefix; fails with exit 5 otherwise.

**In the interactive shell, you are the user.** When you type a command yourself, it runs
immediately regardless of its policy — you have already authorised it by typing it. You will
never be prompted to confirm your own `mkdir` or told that `rm` requires `sudo`.

**These policies apply to the AI agent** (Phase 3 and beyond). When the AI autonomously
issues commands on your behalf, `Confirm` commands pause to ask your permission and `SudoOnly`
commands are denied unless you granted broad authorization via `sudo ask`.

### `sudo` prefix

`sudo` means conscious human authorisation — not Unix credentials. There is no `/etc/sudoers`
and no uid 0. When you prefix a command with `sudo`, it signals deliberate intent:

```
$ sudo rm /tmp/important.txt
```

This is the conventional form to use when you want to be explicit. Without `sudo`, `rm` runs
identically for you as the user.

The `sudo` prefix is one-shot: it applies to exactly the current command and is cleared
immediately after.

### `sudo ask` — granting the agent broad authorization

To allow the AI agent to perform elevated operations during a single `ask` invocation:

```
$ sudo ask "clean up all the build artifacts and temporary files"
```

This grants the agent authorization to run `Confirm` and `SudoOnly` commands for that
invocation only. After `ask` returns, the authorization is cleared.

---

## 10. `export --secret`

Use `export --secret` to set environment variables that should never appear in the
transcript or be readable through `/proc/<pid>/environ`:

```
$ export --secret GITHUB_TOKEN=ghp_xxxx
$ echo $GITHUB_TOKEN
ghp_xxxx
```

The variable is usable in the shell session normally, but the name `GITHUB_TOKEN` is
registered in the secrets registry. It will not appear in transcript output and is filtered
from `/proc/<pid>/environ`.

---

## 11. `prompt-user` — the AI asks you a question

`prompt-user` is a shell builtin designed for use by the AI model. When the model needs
clarification or approval before proceeding, it runs `prompt-user` and waits for your
response.

You can also use it directly:

```
$ prompt-user "Which branch should I merge into?"
Which branch should I merge into?
> main
main
```

The response is written to stdout, so the model can capture it:

```sh
BRANCH=$(prompt-user "Which branch should I merge into?")
git merge $BRANCH
```

### Constrained choices

```
$ prompt-user --choices yes,no "Are you sure?"
Are you sure?
Options: yes, no
> yes
yes
```

The shell loops until the user types one of the listed options (case-insensitive).

### Confirmation shorthand

```
$ prompt-user --confirm "Deploy to production?"
Deploy to production?
Options: yes, no
> no
no
```

`--confirm` is equivalent to `--choices yes,no`.

### Secret input

```
$ DB_PASS=$(prompt-user --secret "Enter database password:")
Enter database password:
> 
```

With `--secret`, terminal echo is suppressed while you type. The value is captured but
never enters the transcript.

### Markdown context

Pipe Markdown into `prompt-user` to have it rendered as readable text before the prompt:

```sh
git diff HEAD | ask "summarise these changes" | prompt-user --confirm "Apply this patch?"
```

The piped content is rendered with tables, emphasis, and code blocks before the question
is shown.

---

## 12. What does not work yet

These features are planned but not yet implemented:

| Feature | When |
|---|---|
| `ask repl` (persistent conversation session) | Phase 4 |
| MCP tool integration | Phase 3 |
| Package installation (`grease`) | Phase 3 |
| Golem cloud deployment | Phase 4 |
| `model remove`, `model info` | Planned |

Everything else described in this tutorial — `ls`, `cat`, `grep`, `stat`, `mkdir`, `rm`,
`touch`, `env`, `ps`, all `ask` flags, all `context` subcommands, all four model providers,
`model add`, `model default`, `model list`, `prompt-user`, authorization, `export --secret`,
and the virtual `/proc/` filesystem — is fully implemented and working.

---

## 13. Keyboard shortcuts

| Key | Effect |
|---|---|
| `Ctrl-D` | Exit the shell (EOF) |
| `Ctrl-C` | Cancel the current line |
| `↑` / `↓` | Command history (if readline is available) |

---

## 14. Tips

**Check what went into the transcript before asking:**
Run `context show` before `ask` to see exactly what context the model will receive.

**Clear before a new task:**
If you have been working on something unrelated, `context clear` before starting a new
task gives the model a clean view.

**Use `--fresh` in scripts:**
Scripts that use `ask` for a single focused question should use `--fresh` to avoid
accidentally including unrelated session history.

**Token budget:**
The transcript has a default budget of approximately 100,000 tokens. For very long sessions
you will not notice any degradation — `context show` will always display the full history,
but `ask` uses only the most recent portion that fits within the budget.
