---
title: Command output is buffered — not forwarded to terminal until command completes
date: 2026-03-07
author: agent
---

## Problem

The current `run_command()` implementation in `ClankShell` buffers all command
output via a pipe and only writes it to the terminal after the command completes.
For short-lived commands like `echo` or `ls` this is imperceptible. For any
command that produces output over time — a long-running build, a streaming
response, a script with multiple steps — the user sees nothing until the very
end.

This is a real regression from the behaviour of a normal shell, where output
appears on the terminal as it is produced.

## Root Cause

`run_command()` creates a `std::io::pipe()`, captures all output into a buffer,
drops the params (closing the write end), then reads the full buffer and writes
it to stdout in one shot. There is no concurrent forwarding — capture and
display are sequential.

## What Is Required

A "tee" mechanism: as bytes arrive on the read end of the pipe, they are
simultaneously:

1. Written to the real terminal stdout/stderr (so the user sees output in
   real-time)
2. Accumulated in a buffer (for recording in the transcript after completion)

This requires a background drain thread (or async task) that reads from the
pipe's read end, forwards to the real terminal, and accumulates into a buffer.
After the command completes (write end is dropped → EOF on pipe), the drain
thread finishes and the accumulated buffer is returned for transcript recording.

## Acceptance Condition

- A command that produces output over multiple seconds (e.g. a script with
  `sleep` between echo calls) displays each line as it is produced, not all
  at once at the end.
- The transcript still receives the complete output after the command finishes.
- All existing tests continue to pass.
