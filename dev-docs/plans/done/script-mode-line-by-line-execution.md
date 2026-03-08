---
title: "Script mode line-by-line execution for correct transcript semantics"
date: 2026-03-07
author: agent
issue: "dev-docs/issues/open/script-mode-line-by-line-execution.md"
research:
  - "dev-docs/research/brush-embedding-api.md"
designs:
  - "dev-docs/designs/proposed/transcript-and-context-builtin.md"
---

# Script mode line-by-line execution for correct transcript semantics

## Originating Issue

Script mode executes the entire stdin blob as one unit — transcript, output
capture, and context commands do not work correctly. See
`dev-docs/issues/open/script-mode-line-by-line-execution.md`.

## Research Consulted

`dev-docs/research/brush-embedding-api.md` — brush-core 0.4.0 embedding API.

Oracle testing research (conducted during issue creation): brush feeds the
entire `stdin` YAML field as a single blob to the shell subprocess. This is
orthogonal to transcript semantics — brush validates scripting language
behaviour against bash, not transcript recording. The clank acceptance harness
model is correct; the problem is inside `run_with_options`.

## Design

### Core insight

`shell.run_string(script, &params)` parses and executes all statements in
`script` as one atomic unit. There is no per-statement callback. To achieve
per-statement recording (command text + output + post-execution recording),
the script must be parsed into individual top-level statements first, and
`run_string` called once per statement.

`brush-parser` is already a dependency of `clank-core`. Its `Parser` type
exposes `parse_program() -> Result<ast::Program, ParseError>`. `Program`
contains `complete_commands: Vec<CompleteCommand>`, each corresponding to one
top-level shell statement (simple command, pipeline, compound command, etc.).

### Source text reconstruction from AST

Each `CompleteCommand` implements the `SourceLocation` trait and exposes
`location() -> Option<TokenLocation>`. `TokenLocation` has `start` and `end`
fields of type `Arc<SourcePosition>`. `SourcePosition.index` is the 0-based
character index (not byte offset) into the original source string.

Source text reconstruction for a statement:

```rust
fn stmt_source<'a>(source: &'a str, loc: &TokenLocation) -> &'a str {
    let start = loc.start.index;
    let end = loc.end.index;
    // index is a character index; collect chars to slice correctly.
    let chars: Vec<char> = source.chars().collect();
    // Re-build as a &str by byte-offsetting from the char indices.
    let byte_start = source
        .char_indices()
        .nth(start)
        .map(|(b, _)| b)
        .unwrap_or(0);
    let byte_end = source
        .char_indices()
        .nth(end)
        .map(|(b, _)| b)
        .unwrap_or(source.len());
    source[byte_start..byte_end].trim()
}
```

If `location()` returns `None` (possible for synthetic nodes), fall back to
the full statement string by calling `run_string` with the full source line
and recording `"<unknown>"` as the command text.

In practice, all `CompleteCommand` nodes produced from real source have
locations. The `None` case is a defensive fallback only.

### `run_with_options` rewrite

`run_with_options` is rewritten to use the same per-statement pattern as
`run_interactive`, sharing the logic via a new private function
`run_statement`:

```rust
async fn run_statement(
    shell: &mut Shell,
    params: &ExecutionParameters,
    cmd_text: &str,
) -> Result<ExecutionResult, Error> {
    // pipe capture, run_string, restore FDs, record command + output
}
```

`run_with_options` becomes:

```rust
pub async fn run_with_options(script: &str, options: CreateOptions) -> Result<u8, Error> {
    let mut shell = Shell::new(options).await?;
    let params = shell.default_exec_params();

    let stmts = parse_statements(script)?;   // Vec<String> of stmt source texts
    let mut last_exit_code: u8 = 0;

    for stmt in &stmts {
        let result = run_statement(&mut shell, &params, stmt).await?;
        last_exit_code = result.exit_code.into();
        if result.is_return_or_exit() {
            break;
        }
    }

    Ok(last_exit_code)
}
```

`run_interactive` is updated to call `run_statement` instead of duplicating
the pipe capture / record logic inline.

### `parse_statements` helper

```rust
fn parse_statements(source: &str) -> Result<Vec<String>, Error> {
    use brush_parser::{Parser, ParserOptions, SourceInfo};
    let reader = std::io::Cursor::new(source);
    let mut parser = Parser::new(reader, &ParserOptions::default(), &SourceInfo::default());
    let program = parser.parse_program()
        .map_err(|e| Error::from(brush_core::ErrorKind::from(e)))?;

    let stmts = program.complete_commands.iter()
        .map(|cmd| {
            cmd.location()
                .map(|loc| stmt_source(source, &loc).to_owned())
                .unwrap_or_else(|| "<unknown>".to_owned())
        })
        .collect();
    Ok(stmts)
}
```

Empty scripts (`stmts` is empty) are a no-op: zero iterations, exit code 0.

### Error mapping from `brush_parser::ParseError`

`brush_core::ErrorKind` has a `ParseError` variant wrapping
`brush_parser::ParseError`:

```rust
ErrorKind::ParseError { .. }
```

The exact mapping needs confirming at implementation time; use `From` impls
available on `ErrorKind` to convert parse errors without leaking parser types
into the public API.

### `run_interactive` refactor

`run_interactive` currently duplicates the pipe capture pattern inline. After
`run_statement` is extracted, `run_interactive` is simplified to call
`run_statement` per line, matching the `run_with_options` structure exactly.
The `SameProcessGroup` params setup and the prompt-writing remain in
`run_interactive`.

### Output capture with tee for script mode

In script mode the process's stdout must still be visible to the user (and
to the acceptance harness). In interactive mode, `run_interactive` restores
`OpenFile::Stdout(std::io::stdout())` after each pipe drain, so the captured
output is printed back via transcript display. For script mode, the same
mechanism applies per statement — capture into a pipe, then write the captured
bytes to the real stdout after draining.

Concretely, after `cap_reader.read_to_string(&mut captured)`:

```rust
// Print captured output to real stdout so it still reaches the terminal.
print!("{captured}");
```

This closes the current gap in script mode: output both reaches the terminal
and enters the transcript.

### `run_statement` signature and shared logic

```rust
async fn run_statement(
    shell: &mut Shell,
    params: &ExecutionParameters,
    cmd: &str,
) -> Result<ExecutionResult, Error> {
    let (mut reader, writer) =
        std::io::pipe().map_err(|e| Error::from(brush_core::ErrorKind::from(e)))?;
    shell.replace_open_files([
        (OpenFiles::STDIN_FD,  OpenFile::Stdin(std::io::stdin())),
        (OpenFiles::STDOUT_FD, OpenFile::PipeWriter(writer)),
        (OpenFiles::STDERR_FD, OpenFile::Stderr(std::io::stderr())),
    ].into_iter());

    let result = shell.run_string(cmd, params).await?;

    shell.replace_open_files([
        (OpenFiles::STDIN_FD,  OpenFile::Stdin(std::io::stdin())),
        (OpenFiles::STDOUT_FD, OpenFile::Stdout(std::io::stdout())),
        (OpenFiles::STDERR_FD, OpenFile::Stderr(std::io::stderr())),
    ].into_iter());

    let mut captured = String::new();
    reader.read_to_string(&mut captured).ok();

    // Echo captured output to real stdout.
    if !captured.is_empty() {
        print!("{captured}");
    }

    // Record command after execution (not visible to itself).
    clank_transcript::global()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .push(TranscriptEntry::command(cmd));

    if !captured.is_empty() && !is_inspection_command(cmd) {
        clank_transcript::global()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(TranscriptEntry::output(captured.trim_end_matches('\n')));
    }

    Ok(result)
}
```

### `ErrorKind` mapping for `ParseError`

At implementation time, confirm that `brush_core::ErrorKind` provides a
`From<brush_parser::ParseError>` impl or an equivalent variant. If not,
wrap with the available `FailedSourcingFile` or `CheckedExpansionError`
variant as a stopgap and file a note in the plan.

### Acceptance test suite update

Once script mode is line-by-line, `context.yaml` can be updated:
- Remove the large comment block explaining the script-mode limitation.
- Add cases that assert `context show` produces output after prior commands.
- Add output capture cases: `echo hello; context show` → stdout contains
  `output: hello`.
- Strengthen `show_output_is_not_re_recorded` to assert that `context show`
  output does not contain `"] output:"`.
- Update `run_with_options_records_command_but_not_output` in
  `clank-core/tests/transcript.rs` to `run_with_options_records_command_and_output`.

### Existing test impact

`clank-core/tests/transcript.rs`:
- `run_with_options_records_command_but_not_output` → renamed and updated to
  assert output is captured.
- All `run_records_command_text` / `run_multiple_commands` tests remain valid
  — they still assert on `command_texts()` which will be populated the same way.
- No changes needed to interactive tests.

`clank-core` unit tests in `src/lib.rs`:
- `echo_hello_exits_zero` and `false_exits_nonzero` are unaffected.

## Developer Feedback

No open design questions. The `brush-parser` API provides source spans
(`TokenLocation.start.index` / `end.index`) sufficient for source text
reconstruction. The `run_statement` extraction makes the implementation
straightforward and eliminates code duplication between `run_with_options`
and `run_interactive`.

## Tasks

- [ ] Add `parse_statements(source: &str) -> Result<Vec<String>, Error>` to
      `clank-core/src/lib.rs` using `brush-parser`; returns per-statement
      source texts extracted via `TokenLocation` char indices
- [ ] Extract `run_statement(shell, params, cmd) -> Result<ExecutionResult, Error>`
      from `run_interactive`'s inline pipe-capture logic; add stdout echo
      after capture so output reaches the terminal
- [ ] Rewrite `run_with_options` to call `parse_statements` then iterate
      `run_statement` per statement, breaking on `is_return_or_exit()`
- [ ] Simplify `run_interactive` to call `run_statement` per line (removing
      the now-duplicated pipe-capture block)
- [ ] Update `run_with_options_records_command_but_not_output` integration
      test to assert output entries are now present
- [ ] Add integration test: multi-statement script (`echo a\necho b`) records
      two separate `Command` + `Output` entry pairs
- [ ] Add integration test: `context show` in a script sees prior commands
      recorded by preceding statements in the same script
- [ ] Update `context.yaml` acceptance tests: remove script-mode limitation
      comment block; add cases asserting `context show` shows prior commands;
      add output capture cases; strengthen `show_output_is_not_re_recorded`
- [ ] Run full test suite; verify no regressions

## Acceptance Tests (additions / changes)

In `clank-acceptance/cases/builtins/context.yaml`:

- `echo hello; context show` → `expect_stdout_contains: "command: echo hello"`
- `echo hello; context show` → `expect_stdout_contains: "output: hello"`
- `echo a; echo b; context show` → contains both `command: echo a` and
  `command: echo b`
- `echo a; context clear; context show` → `expect_stdout: ""` (clear wipes
  prior entries; show sees empty)
- `echo a; echo b; context trim 1; context show` → contains `command: echo b`
  but not `command: echo a`
- `context show; context show` → second show contains `command: context show`
  from the first; output does not contain `"] output:"` (non-re-entry)

## Out of Scope

- Multi-line compound commands (`if/fi`, `for/done`, `while/done`, function
  definitions) — these are single `CompleteCommand` AST nodes and are recorded
  as one entry. Splitting them across lines would produce misleading transcript
  entries.
- Changing the brush oracle test harness or its execution model.
- `is_return_or_exit()` behaviour change — script mode already respects this;
  the refactor preserves it.
- Argv mode in `main.rs` — joins args as one string and calls
  `run_with_options`; will automatically benefit from the fix.
