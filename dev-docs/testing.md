# Testing Reference

Tests are organised in three layers.

## 1. Unit tests — inline `#[cfg(test)]` modules

Live in the same file as the code under test. Used for:

- Pure logic (error types, data transformations, Display impls)
- Constructor smoke tests
- Anything that does not require the public API boundary

## 2. Integration tests — `crate/tests/*.rs`

Compiled as a separate binary; access to the crate's public API only. Used for:

- `clank-core/tests/` — `Repl` behaviour driven through injectable I/O (`Cursor<&[u8]>` as
  input, `Vec<u8>` as prompt output)
- `clank-http/tests/` — `HttpClient` trait contract tests using `MockHttpClient`

**`MockHttpClient`** is defined in `clank-http/tests/http_client.rs`. It records every call and
returns a canned response. Copy or re-export it when writing tests that need HTTP without a
real network.

## 3. Golden test matrix — `clank-shell/tests/golden/`

Declarative, file-based end-to-end tests. Each subdirectory is a test case:

```
clank-shell/tests/golden/
  <case-name>/
    stdin       — input fed verbatim to clank's stdin (required; hand-authored)
    stdout      — expected stdout (golden file; auto-updatable)
    exit_code   — expected exit code as a plain integer string (golden file; auto-updatable)
```

Each case appears as a distinct named test in `cargo test` output
(e.g. `golden::golden::echo_hello ... ok`), registered at runtime by `test-r`'s `#[test_gen]`.
Golden file diffing and auto-update are provided by the `goldenfile` crate.

**Adding a case:** create a new directory with a `stdin` file, then run:

```bash
UPDATE_GOLDENFILES=1 cargo test --test golden
```

This populates `stdout` and `exit_code` from the binary's actual output. Review `git diff
tests/golden/` and commit the changes.

**Updating expectations** after an intentional behaviour change:

```bash
UPDATE_GOLDENFILES=1 cargo test --test golden
git diff clank-shell/tests/golden/   # review changes
git add clank-shell/tests/golden/    # commit
```

**Binary path:** use `env!("CARGO_BIN_EXE_clank")` — consistent with the harness in `golden.rs`.

## Injectable I/O contract

`Repl::run()` accepts `impl BufRead` (command input) and `impl Write` (prompt output). Command
output from brush-core builtins flows through the real process stdout/stderr and is not captured
by these parameters. To assert on command output, use layer 3 (golden tests).

## Known limitations

- `exit N` with a non-zero argument does not propagate the exit code to the OS in the current
  brush-core integration. No golden case tests this; it is tracked as an open limitation pending
  a process-model redesign.
