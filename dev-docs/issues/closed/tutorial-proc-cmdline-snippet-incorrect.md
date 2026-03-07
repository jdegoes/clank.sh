---
title: "Tutorial §8 /proc/ snippet is incorrect — PIDs are reaped before next command"
date: 2026-03-07
author: agent
---

# Tutorial §8 `/proc/` snippet is incorrect

## Observed behaviour

The tutorial (section 8) shows:

```
$ cat /proc/1/cmdline
context show
```

This does not work. Running `cat /proc/1/cmdline` as a separate command produces:

```
cat: /proc/1/cmdline: No such file or directory
```

## Root cause

Processes are reaped synchronously at the end of each `run_line` call. By the time
`cat /proc/1/cmdline` runs, PID 1 (whatever ran previously) has already been reaped and
removed from the process table. The `/proc/<pid>/` directory no longer exists.

The `/proc/` VFS is a live view of the process table — it only shows processes that are
currently running or paused. In the interactive REPL, processes complete before the next
prompt appears, so there is never a window to read `/proc/<pid>/cmdline` from a subsequent
command for an already-completed process.

## When /proc/ IS readable

`/proc/<pid>/` entries are accessible while a process is in `P` (Paused) state — i.e.
during a `prompt-user` invocation. A script that runs a command and then immediately reads
its `/proc/` entry while it is still alive would also work. The compound form
`cmd & cat /proc/$!/cmdline` is the idiomatic shell pattern.

## Fix required

1. Correct the tutorial snippet in section 8 to reflect when `/proc/` entries are actually
   readable. Options:
   a. Show the `/proc/` read happening in a script alongside a still-running process.
   b. Explain that `/proc/` reflects live processes and is most visible during `prompt-user`.
   c. Remove the `cat /proc/1/cmdline` example and replace with a note about when to use it.

2. Add a scenario fixture for the corrected snippet so this class of tutorial inaccuracy
   is caught automatically in future.

## Methodology note

This bug was not caught by the tutorial conformance scenario suite because the plan
classified the `/proc/` snippet as "partially automatable — PIDs are non-deterministic"
and skipped it. The correct approach is to attempt a fixture for every deterministic
tutorial snippet and let the failure surface the bug. Skipping a snippet because it
"looks hard" is the gap that allowed this to reach manual testing.
