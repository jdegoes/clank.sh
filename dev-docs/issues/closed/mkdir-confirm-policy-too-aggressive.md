---
title: "mkdir Confirm authorization policy is too aggressive"
date: 2026-03-07
author: agent
---

# mkdir Confirm authorization policy is too aggressive

## Observed behaviour

Running `mkdir demo` prompts the user for confirmation before creating the directory.
This breaks the basic tutorial workflow and makes routine directory creation unnecessarily
cumbersome.

## Root cause

`mkdir` is registered with `AuthorizationPolicy::Confirm` in the manifest alongside `cp`,
`mv`, `touch`, `tee`, and `patch`. While write operations that modify existing content
warrant confirmation (e.g. `cp`, `mv`, `patch`), creating a new directory is a low-risk,
easily reversible operation that does not require user confirmation. The policy grouping was
too broad.

## Fix

Move `mkdir` and `touch` to `AuthorizationPolicy::Allow`. They create new content but do
not modify or overwrite existing files, making confirmation unnecessary.

`cp`, `mv`, `tee`, and `patch` should remain `Confirm` as they can overwrite existing
content.
