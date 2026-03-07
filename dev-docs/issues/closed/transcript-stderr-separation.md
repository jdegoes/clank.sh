---
title: Transcript merges stdout and stderr — error output is indistinguishable
date: 2026-03-07
author: agent
---

## Problem

The current `run_command()` implementation captures both stdout and stderr
through the same pipe writer (by cloning the writer). Both streams arrive
merged in a single `TranscriptEntry::Output` entry. There is no way to
distinguish normal output from error output in the transcript.

This matters for `ask`: when the AI reads the transcript and sees output from
a failing command, it cannot tell whether what it is reading was stdout (the
result) or stderr (the error message). This reduces the AI's ability to
diagnose problems correctly.

## What Is Required

Stdout and stderr must be captured on separate pipes and recorded as separate
transcript entries:

- `TranscriptEntry::Output { text }` — stdout
- `TranscriptEntry::Error { text }` — stderr

`run_command()` must:
1. Create two separate pipes — one for stdout, one for stderr.
2. Capture each stream independently.
3. Append `Output` and `Error` entries to the transcript separately (only if
   non-empty).

This also aligns with the `render()` format: `[output]` vs `[error]` gives the
AI model an unambiguous signal about which stream each entry came from.

## Acceptance Condition

- After running a command that writes to both stdout and stderr, the transcript
  contains two separate entries: one `Output` and one `Error`.
- Empty stdout or stderr produces no entry (existing behaviour preserved).
- `render()` labels them `[output]` and `[error]` respectively.
- All existing tests continue to pass.
