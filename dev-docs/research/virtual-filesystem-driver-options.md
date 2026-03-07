---
title: "Virtual Filesystem Driver Options"
date: 2026-03-06
author: agent
---

# Virtual Filesystem Driver Options

## Motivation

clank.sh requires virtual read-only namespaces that look like filesystem paths to programs but are
not backed by real files:

- `/proc/` — process table, per-process `cmdline`/`status`/`environ`, `/proc/clank/system-prompt`
- `/bin/` — special builtins namespace (not file-backed)
- `/mnt/mcp/<server>/` — MCP resources (static files, dynamic virtual files, resource template stubs)

The README states: no FUSE dependency, no OS support required, "implemented at the shell level."
The implementation must work inside a single `wasm32-wasip2` component where there is no kernel,
no mount table, and no OS support for filesystem virtualization.

This research surveys the viable approaches.

## Constraints

1. Must work on both `wasm32-wasip2` and native Rust without different behavior.
2. No FUSE. No OS-level mounts.
3. Must be transparent to commands inside the shell (e.g. `cat /proc/1/cmdline`,
   `grep "TODO" /mnt/mcp/github/src/main.rs` must work without those commands knowing they are
   reading virtual content).
4. Standard tools (`cat`, `grep`, `ls`, `stat`) must work against virtual paths.
5. Read-only for `/proc/` and `/bin/`. `/mnt/mcp/` has mixed semantics (static files are
   write-once at install/refresh time; dynamic files invoke MCP `resources/read` on each read).

## Candidate Approaches

### Approach A: Intercept at the Brush I/O layer

Brush uses the Rust standard library (`std::fs`) for filesystem operations. It is possible —
depending on Brush's architecture — to replace or wrap the I/O calls at the `brush-core` level
with a custom VFS layer that intercepts path lookups and routes virtual paths to handlers before
falling through to the real filesystem.

**How it works:**
- Define a `Vfs` trait with methods mirroring `std::fs` (`read`, `read_dir`, `stat`, `open`).
- Provide a `LayeredVfs` that checks a path prefix table first (virtual mounts), then delegates to
  the real filesystem.
- Replace all `std::fs` calls in Brush core with `Vfs` calls. (Requires Brush to support this
  injection, or requires a fork/patch of Brush.)

**Pros:**
- Works identically on native and WASM — no OS involvement.
- Full control over what every virtual path returns.
- Dynamic virtual files (MCP `resources/read` on each read) are easy to implement as closures.
- No external dependencies.

**Cons:**
- Requires Brush to expose an I/O abstraction hook or be patched to use one. If Brush uses
  `std::fs` directly without injection points, this requires a non-trivial fork.
- Tools spawned as subprocesses in native mode (if any) would bypass the VFS entirely since they
  operate on the real OS filesystem. However, in clank.sh, all "processes" are synthetic (no real
  `fork`/`exec`), so all I/O goes through the shell's own Rust code — this bypass problem does not
  apply.

**Feasibility:**
Requires checking whether Brush's `brush-core` uses `std::fs` directly or has an abstraction seam.
If it does not, this approach requires either a Brush fork or a shim via Rust's
`#[link_name]`/`LD_PRELOAD`-equivalent — both undesirable.

An alternative within this approach: implement virtual paths not at the Brush I/O layer but at the
**builtin execution layer**. Since `cat`, `grep`, `ls`, `stat` are all core commands implemented
inside clank.sh (not shell scripts invoking real OS binaries), their implementations can call the
VFS directly. The VFS is only needed where clank's own command implementations do I/O — not at the
Brush scripting layer itself.

This is a narrower and more tractable version of Approach A. The key question is whether any
Brush-layer scripting construct (e.g. `<` redirection, `$(<file)`) reads from the filesystem
directly, which would need to go through the VFS.

### Approach B: Real files synthesized at runtime

Instead of intercepting I/O, write the content of virtual paths to real files in a dedicated
directory (e.g. `/tmp/.clank/proc/`) and symlink or bind-mount them to the expected paths. Update
the real files whenever the underlying data changes.

**How it works:**
- At shell startup, create real files under `/tmp/.clank/proc/` with the initial process state.
- Update them on each state change (new process, state transition, etc.).
- `/proc/` in the shell's path namespace resolves to `/tmp/.clank/proc/` via a symlink or
  `$PATH`-equivalent resolution hack.

**Pros:**
- No Brush modification required — `std::fs` reads go to real files.
- No VFS abstraction needed.

**Cons:**
- WASM targets typically do not have a persistent writable filesystem outside of what WASI
  preopens. Depending on Golem's WASI configuration, `/tmp/` may or may not exist.
- Dynamic MCP resources (whose content must be fetched live on each read) cannot be satisfied by
  pre-written files — you would need to poll and re-write, which is racy and resource-intensive.
- Maintaining consistency between shell state and file content requires careful synchronization.
- `/proc/clank/system-prompt` is explicitly "computed on read" — this is incompatible with a
  pre-written file unless the file is updated on every state change that affects the system prompt,
  which is every `grease install/remove`. Tractable but fragile.
- `/bin/` as a virtual read-only namespace is awkward to fake with real files.

**Verdict:** Not recommended. Dynamic virtual files (computed on read) are a first-class
requirement and this approach cannot satisfy them cleanly.

### Approach C: VFS implemented as a Brush extension via custom builtin dispatch

Brush's `brush-core` provides an API for registering custom builtins. The core commands in
clank.sh (`cat`, `ls`, `grep`, `stat`, etc.) are already implemented as builtins or subprocess
implementations inside the shell, not as real OS commands. The VFS can be implemented entirely
within those command implementations.

**How it works:**
- `cat`, `ls`, `grep`, `head`, `tail`, `stat`, `file`, etc. are implemented as clank commands.
- Each implementation calls a central `Vfs::open(path)` / `Vfs::read_dir(path)` function.
- `Vfs` checks a mount table for virtual path prefixes first, dispatches to a handler, and falls
  through to `std::fs` for real paths.
- The mount table is populated at startup: `/proc` → `ProcHandler`, `/mnt/mcp/<server>` →
  `McpResourceHandler`, `/bin` → `BinHandler`.
- Shell I/O redirections (`< /proc/1/cmdline`, `$(<file)`) are handled by a custom I/O handler
  registered with Brush — this is the only place Brush-internal I/O needs to be intercepted.

**Pros:**
- Does not require patching Brush's core filesystem access (only needs a hook for I/O redirections,
  which Brush may already support via its extension API).
- All clank command implementations go through the VFS — transparent to callers.
- Dynamic virtual files are natural: `McpResourceHandler::read()` calls `resources/read` and
  returns the bytes.
- Works identically on native and WASM.

**Cons:**
- Brush I/O redirections (`< /file`) still need to be intercepted. If Brush does not expose a hook
  for redirecting I/O open operations, scripts that use `< /proc/1/cmdline` would read from the
  real filesystem (and fail). This is the single critical unknown.
- The VFS is only as complete as the set of commands that use it — any clank command that calls
  `std::fs` directly bypasses it.

**Feasibility:**
Highly tractable if Brush exposes an I/O open hook. Needs a spike against the Brush source to
determine whether such a hook exists or can be added cleanly. This is the recommended approach
pending that investigation.

### Approach D: `/proc/` not as real filesystem paths, but as command aliases

Rather than making `/proc/1/cmdline` a readable file, expose the same information through
commands: `ps --pid 1 --format cmdline`, `clank status <pid>`, etc. Scripts that want to inspect
process state call these commands rather than reading files.

**Pros:**
- Zero VFS complexity.
- No Brush modification.
- Works trivially on WASM.

**Cons:**
- Directly contradicts the specification. The README explicitly requires `/proc/<pid>/cmdline`,
  `/proc/<pid>/status`, `/proc/<pid>/environ`, and `/proc/clank/system-prompt` as readable files,
  accessible to standard tools (`cat`, `grep`). This approach is non-conforming.

**Verdict:** Rejected. Non-conforming with spec.

## Recommendation

**Approach C (VFS at the command implementation layer)** is the recommended approach, subject to a
research spike on whether Brush exposes an I/O open hook for redirections.

The spike should answer:
1. Does `brush-core` expose a hook for customizing how I/O redirections open files (the equivalent
   of a custom `open(2)` at the shell level)?
2. If not, how invasive would it be to add one?
3. Does `brush-core` use `std::fs` directly in any scripting construct that could reach virtual
   paths (e.g. `[[ -f /proc/1/cmdline ]]` test expressions)?

If Brush does not expose such a hook and the addition is invasive, a hybrid of A and C is the
fallback: implement the VFS in command implementations (Approach C) and accept that I/O
redirections to virtual paths will not work in scripts — a known limitation to document and address
in a future release.

## Virtual Path Mount Table (design sketch)

```
/proc              → ProcHandler           (read-only; computed from process table)
/bin               → BinHandler            (read-only; virtual namespace for special builtins)
/mnt/mcp/<server>  → McpResourceHandler    (mixed: static files real; dynamic files fetch on read)
```

Each handler implements:
```rust
pub trait VfsHandler: Send + Sync {
    fn read_file(&self, path: &Path) -> Result<Vec<u8>, VfsError>;
    fn read_dir(&self, path: &Path) -> Result<Vec<DirEntry>, VfsError>;
    fn stat(&self, path: &Path) -> Result<FileStat, VfsError>;
}
```

`/proc/clank/system-prompt` is a `ProcHandler` entry whose `read_file` implementation assembles
the system prompt from the current installed tool manifests and shell configuration at call time.

## References

- `README.md` §"Process Model" — `/proc/` specification
- `README.md` §"MCP Server Interaction" — resource mount layout
- `README.md` §"Filesystem" — full directory tree
- `README.md` §"Architecture" — "virtual read-only namespace... no FUSE dependency"
- https://github.com/reubeno/brush — Brush shell interpreter (source to be reviewed for I/O hooks)
