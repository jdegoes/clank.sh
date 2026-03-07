# clank.sh — Project Overview

## What is clank.sh?

clank.sh is an AI-native shell for Linux. The core idea is simple: a shell already knows
everything you've been doing — every command you've typed, every output you've seen. That's
exactly the context an AI model needs to be useful. Rather than bolting AI onto the side of
a terminal as a separate tool you have to brief, clank.sh makes the AI a first-class citizen
of the shell itself.

You use it like a normal bash-compatible shell. But when you want help, you just type:

```
ask "what's wrong with this output?"
```

The model already sees your entire session. It knows what you ran, what failed, what the
error said. You don't explain anything — it was there.

Every capability — AI prompts, external tools, cloud agents — installs as an ordinary command
on `$PATH`. You extend the shell by installing packages, not by learning a new interface.

The long-term target is for clank.sh to run inside [Golem](https://golem.cloud) as a durable
WebAssembly component, making it a fully persistent agent that survives infrastructure
failures, can be rewound, forked, and left idle at no cost.

---

## What has been built

Four phases of implementation are complete.

**Phase -1** established the foundations: a working Cargo workspace, Brush (a bash-compatible
interpreter) embedded as the shell engine, and an internal process abstraction that intercepts
command dispatch. The shell starts, runs bash commands, handles pipes, redirections, multi-line
constructs, and job control basics.

**Phase 1** added the AI integration. The shell now:

- **Records a session transcript** — every command you type and every output produced is
  captured in a sliding-window transcript. This is what the model reads.
- **`ask` works end-to-end** — `ask "question"` calls the model with your transcript as
  context and streams the response back. The response is appended to the transcript so
  future `ask` calls can see the conversation history.
- **Piped input works** — `cat error.log | ask "summarise this"` sends the file contents
  alongside your question as supplementary input.
- **`context` manages the transcript** — `context show`, `context clear`, `context trim N`,
  `context summarize`.
- **Four model providers** — Anthropic directly, OpenRouter (hundreds of models via a single
  key), Ollama (local), and any OpenAI-compatible server (llama.cpp, LM Studio, vLLM).
- **`model add`** configures providers: `model add anthropic --key <KEY>`,
  `model add ollama`, `model add openai-compat --url http://localhost:8080`.
- **`model list`** shows configured providers and their status.
- **Exit codes, stdout/stderr discipline, and the `--json` contract** all conform to the
  spec.

**Phase 2** built the process model and authorization layer. The shell now:

- **Tracks every command as a synthetic process** — a global process table records PIDs,
  parent PIDs, argv, and lifecycle state for every dispatched command.
- **`ps` works** — `ps`, `ps aux`, and `ps -ef` produce standard-format output.
- **`prompt-user` works** — the AI can pause and ask the human a question. Supports free
  text, constrained choices, confirmation, and secret input.
- **Authorization is enforced** — `Allow`, `Confirm`, and `SudoOnly` policies per command,
  applied to AI agent commands only. The human user always runs commands freely. `sudo`
  is a human gesture: `sudo ask` grants the agent broad authorization for one invocation.
- **`export --secret` works** — secrets are filtered from the transcript and `/proc/environ`.
- **Virtual `/proc/` filesystem** — `cmdline`, `status`, and `environ` are readable via `cat`.
  `env` shows the current shell environment with secrets masked as `***`.

**Infrastructure and quality** — alongside the feature phases, the following foundations
were established:

- **Scenario test harness** — single-file YAML fixtures with built-in config isolation,
  `config_after` assertions, and `CLANK_UPDATE=1` regeneration. Replaced `trycmd`.
- **Comprehensive test coverage** — 80+ new tests covering all seven command implementations,
  `HttpError` display/conversion contracts, `ProcHandler` wire formats, `Vfs` operations,
  authorization enforcement, and `AskProcess` transcript append contract.
- **`Vfs` write operations** — `mkdir`, `touch`, and `rm` use the `Vfs` abstraction rather
  than calling `std::fs` directly, enabling mock testing and future WASM portability.
- **Typed error enums** — `AskFlagError` and `ProviderSelectError` replace stringly-typed
  error returns throughout `ask_process.rs`.
- **Per-shell authorization state** — sudo state is now keyed by shell ID, preventing
  bleed between concurrent shell instances in tests and production.

---

## What remains

Four phases of feature work remain before the project is production-ready.

**Phase 3 — Packages and MCP** (~large effort)
The `grease` package manager for installing prompts, tools, and scripts. Integration with
MCP servers. Virtual `/mnt/mcp/<server>/` filesystem. Tab completion driven by installed
manifests. Real implementations of remaining core Unix commands. This is the phase where
the shell becomes genuinely extensible.

**Phase 4 — Golem integration** (~large effort)
The Golem cloud adapter — durable state, exactly-once tool calls, agent lifecycle management.
The `golem` command. `ask repl` for sustained AI conversation sessions.

**Phase 5 — Polish** (~medium effort)
Signed package registry, automatic transcript compaction, full TUI, `man` pages, MCP
authentication, system prompt iteration.

The following open issues are tracked in `dev-docs/issues/open/` and must be addressed
before Phase 3 ships:

- **`sudo ask` broad authorization** — `sudo ask "..."` is accepted but the broad-authorization
  grant is not propagated to agent-issued commands. Design and implementation deferred to
  Phase 3 alongside agent command dispatch.
- **`/proc/<pid>/cmdline` tutorial example** — the tutorial section on `/proc/` has been
  corrected; the per-process `/proc/` entries are readable only during `P` (Paused) state.

---

## How complete is it?

Roughly **40% of the total feature surface**. The foundations are solid, well-tested, and
clean — the transcript, AI integration, provider layer (including local models), process
model, authorization system, `prompt-user`, virtual filesystem, and `env`/`ps`/`cat`/`ls`/
`grep`/`stat` commands are all done and conforming to the spec.

What remains is additive: the package system, MCP integration, and cloud deployment on top
of a working, high-quality base.

The shell is usable today for `ask` interactions and simple agentic workflows. It supports
both cloud providers (Anthropic, OpenRouter) and local models (Ollama, llama.cpp, LM Studio)
out of the box. It is not yet suitable for serious extensible agentic work — that requires
Phase 3's package system and MCP integration.

---

*For installation and usage instructions, see [TUTORIAL.md](TUTORIAL.md).*
*For the full design specification, see [README.md](README.md).*
