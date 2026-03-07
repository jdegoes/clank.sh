---
title: "Phase 2 Prerequisite Spikes: Brush I/O Hook and export --secret"
date: 2026-03-06
author: agent
---

# Phase 2 Prerequisite Spikes

Two questions needed answers before the Phase 2 plan could be finalised.

---

## Spike 1: Brush I/O redirection hook

### Question

Does brush-core 0.4.0 expose a hook allowing an embedder to intercept how I/O redirections
open files? If not, what is the least invasive change to add one?

### Answer

**No hook exists in 0.4.0.** All I/O redirection file opens converge at a single function:

```
Shell::open_file(&self, options: &OpenOptions, path: impl AsRef<Path>, params: &ExecutionParameters)
    → brush-core/src/shell.rs, lines 1377–1401
```

This function calls `std::fs::OpenOptions::open()` directly with no abstraction layer.
The function is `pub(crate)` — not accessible to embedders.

### The call chain

For `cat < /proc/1/cmdline`:
```
setup_redirect (interp.rs:1353)
  → Shell::absolute_path
  → Shell::open_file (shell.rs:1377)
      → OpenOptions::open("/proc/1/cmdline")   ← bare fs call, no hook
  → params.open_files.set_fd(0, OpenFile::File)
```

For `[[ -f /proc/1/cmdline ]]`: uses `Path::is_file()` / `fs::metadata()` directly in
`extendedtests.rs` — **completely separate code path, no `Shell::open_file` involved**.

The `interfaces.rs` module exports only `KeyBindings`. There is no `FileSystem`, `VirtualFs`,
or `OpenHook` trait.

### Minimal upstream change to add a hook

The entire redirection open path converges at `Shell::open_file`. A minimal PR to brush-core
would add an optional `OpenFileHook` trait to `interfaces.rs` and consult it in
`Shell::open_file` before the bare `options.open()` call:

```rust
pub trait OpenFileHook: Send + Sync {
    fn open(
        &self,
        path: &Path,
        options: &std::fs::OpenOptions,
    ) -> Result<Option<OpenFile>, std::io::Error>;
}
```

`Shell` gains `open_file_hook: Option<Arc<dyn OpenFileHook>>`. In `open_file()`:

```rust
if let Some(hook) = &self.open_file_hook {
    if let Some(virtual_file) = hook.open(&path_to_open, options)? {
        return Ok(virtual_file);
    }
}
Ok(options.open(path_to_open)?.into())
```

This touches exactly two files in brush-core: `shell.rs` and `interfaces.rs`. It would
also require a `Virtual(Box<dyn Read + Send>)` variant in the `OpenFile` enum, or using
`std::io::pipe()` as a transport (simpler: hook writes content into a pipe write end,
returns the read end as `OpenFile::PipeReader`).

The `-f`/`-e`/`-d` test expressions in `extendedtests.rs` are a **separate code path**
and would require a second hook to virtualise.

### Implication for Phase 2

`/proc/` paths used in I/O redirections (`cat < /proc/1/cmdline`, `$(<file)`) will **not**
work in Phase 2 unless we upstream this hook or maintain a local fork. The plan recommends:

1. File the upstream PR in parallel with Phase 2 implementation.
2. In Phase 2, implement `/proc/` virtualisation only at the command implementation layer
   (Approach C from the VFS research doc): `cat`, `ls`, `stat` etc. call the VFS directly.
3. Document that `< /proc/...` redirections in scripts are a known limitation in Phase 2.
4. If the upstream PR lands, add the hook in a follow-up. If it doesn't, reassess forking
   at Phase 3.

---

## Spike 2: `export --secret` override mechanism

### Question

How do we add `--secret` flag support to Brush's native `export` special builtin without
forking Brush?

### Answer

Brush's `export` is implemented in `brush-builtins/src/export.rs` as `ExportCommand`,
which implements `builtins::DeclarationCommand` and `builtins::Command`. It is registered
in the default builtin set.

The registration API allows replacing any named builtin. Since clank.sh already registers
all commands via `Shell::builder().builtins(map)`, registering `"export"` in the clank
builtin map **overrides** Brush's default `export` with our own implementation. No fork
required.

Our `ExportCommand` needs to:
1. Accept `--secret` as an additional flag (alongside Brush's existing `-f`, `-n`, `-p`).
2. When `--secret` is present, set the variable as exported **and** tag it as sensitive in
   a side-channel (a `HashSet<String>` of secret variable names stored in the shell global
   state or a dedicated `SecretsRegistry`).
3. For non-`--secret` invocations, replicate Brush's existing `export` behaviour exactly
   (set/unset export flag, handle assignments, handle `-p`). We can copy the logic from
   the brush-builtins source.

The sensitive tag must be checked:
- In `env` output (suppress the value, show `***` or omit entirely)
- In `ps` / `/proc/<pid>/environ` (suppress)
- In transcript append (never append `Output` entries containing secret values —
  enforced by redaction rules in the manifest, Phase 2+ concern)
- In logs

### Sensitive variable registry

A `SecretsRegistry` is a `HashSet<String>` of variable names marked as secret via
`export --secret`. It lives in `ClankShell` (or the global dispatch state) and is
consulted by:
- The `env` command implementation
- The `/proc/<pid>/environ` VFS handler
- The `ps` command implementation (suppress env values)

### Implementation approach for Phase 2

Register `"export"` in `clank_builtins()` with a `ClankExportProcess` implementation.
`ClankExportProcess` intercepts `--secret`, delegates all other flags to the same logic
as Brush's `ExportCommand` (reimplemented in ~80 lines following the brush-builtins source),
and records secret variable names in a global `SecretsRegistry`.

This is declaration-builtin behaviour — `declaration_builtin: true` must be set in the
`Registration` to ensure Brush routes assignment-style arguments (`KEY=value`) correctly.
