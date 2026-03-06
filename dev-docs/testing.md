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

## 3. End-to-end binary tests — `clank-shell/tests/acceptance.rs`

Drives the compiled `clank` binary via `std::process::Command` using `assert_cmd` and
`predicates`. Used for acceptance criteria from plans and binary-level regression tests.

**Binary path:** use `env!("CARGO_BIN_EXE_clank")` — do not use the deprecated
`assert_cmd::cargo::cargo_bin` helper.

## Injectable I/O contract

`Repl::run()` accepts `impl BufRead` (command input) and `impl Write` (prompt output). Command
output from brush-core builtins flows through the real process stdout/stderr and is not captured
by these parameters. To assert on command output, use layer 3.

## Known limitations

- `exit N` with a non-zero argument does not propagate the exit code to the OS in the current
  brush-core integration. The corresponding test is commented out in `acceptance.rs` pending a
  process-model redesign.
