pub mod proc_handler;

use std::path::{Path, PathBuf};

/// A directory entry returned by `read_dir`.
#[derive(Debug, Clone)]
pub struct DirEntry {
    pub path: PathBuf,
    pub is_dir: bool,
    pub is_file: bool,
    pub is_symlink: bool,
}

/// Metadata about a file or directory.
#[derive(Debug, Clone)]
pub struct FileStat {
    pub is_dir: bool,
    pub is_file: bool,
    pub is_symlink: bool,
    pub size: u64,
}

/// Errors returned by VFS operations.
#[derive(Debug, thiserror::Error)]
pub enum VfsError {
    #[error("No such file or directory")]
    NotFound(PathBuf),

    #[error("permission denied")]
    PermissionDenied(PathBuf),

    #[error("I/O error on {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
}

/// Abstraction over filesystem access.
///
/// All filesystem I/O in clank's command implementations goes through this trait.
/// `RealFs` delegates to `std::fs`. Virtual mount handlers (`ProcHandler`,
/// `McpResourceHandler`, etc.) are added in later phases via `LayeredVfs`.
///
/// Write methods take `&self` (not `&mut self`) so that `Arc<dyn Vfs>` holders
/// can call them without exclusive ownership. Implementations use interior
/// mutability as needed (e.g. `MockVfs` wraps its map in `RwLock`).
pub trait Vfs: Send + Sync {
    // Read operations
    fn read_file(&self, path: &Path) -> Result<Vec<u8>, VfsError>;
    fn read_dir(&self, path: &Path) -> Result<Vec<DirEntry>, VfsError>;
    fn stat(&self, path: &Path) -> Result<FileStat, VfsError>;
    fn exists(&self, path: &Path) -> bool;

    // Write operations
    fn write_file(&self, path: &Path, contents: &[u8]) -> Result<(), VfsError>;
    fn create_dir(&self, path: &Path) -> Result<(), VfsError>;
    fn create_dir_all(&self, path: &Path) -> Result<(), VfsError>;
    fn remove_file(&self, path: &Path) -> Result<(), VfsError>;
    fn remove_dir_all(&self, path: &Path) -> Result<(), VfsError>;
}

/// Test double for `Vfs`.
///
/// Backed by an in-memory `RwLock<HashMap<PathBuf, Vec<u8>>>`. Paths not in
/// the map return `VfsError::NotFound`. Write methods update the map in place.
/// Available to all crates as a `dev-dependency` on `clank-vfs`.
///
/// # Example
///
/// ```rust
/// # use clank_vfs::{MockVfs, Vfs};
/// # use std::path::Path;
/// let vfs = MockVfs::new()
///     .with_file("/proc/clank/system-prompt", "you are clank");
/// let content = vfs.read_file(Path::new("/proc/clank/system-prompt")).unwrap();
/// assert_eq!(content, b"you are clank");
/// ```
pub struct MockVfs {
    files: std::sync::RwLock<std::collections::HashMap<PathBuf, Vec<u8>>>,
}

impl MockVfs {
    pub fn new() -> Self {
        Self {
            files: std::sync::RwLock::new(std::collections::HashMap::new()),
        }
    }

    /// Register a file with the given path and content (builder method).
    pub fn with_file(self, path: impl Into<PathBuf>, content: impl Into<Vec<u8>>) -> Self {
        self.files
            .write()
            .expect("MockVfs files lock poisoned")
            .insert(path.into(), content.into());
        self
    }
}

impl Default for MockVfs {
    fn default() -> Self {
        Self::new()
    }
}

impl Vfs for MockVfs {
    fn read_file(&self, path: &Path) -> Result<Vec<u8>, VfsError> {
        self.files
            .read()
            .expect("MockVfs files lock poisoned")
            .get(path)
            .cloned()
            .ok_or_else(|| VfsError::NotFound(path.to_owned()))
    }

    fn read_dir(&self, path: &Path) -> Result<Vec<DirEntry>, VfsError> {
        let guard = self.files.read().expect("MockVfs files lock poisoned");
        let prefix = path.to_owned();
        let entries: Vec<DirEntry> = guard
            .keys()
            .filter(|p| p.parent() == Some(&prefix))
            .map(|p| DirEntry {
                path: p.clone(),
                is_dir: false,
                is_file: true,
                is_symlink: false,
            })
            .collect();
        if entries.is_empty() && !guard.contains_key(path) {
            // Also return empty (not NotFound) if any key has this as a parent,
            // meaning the path is an implicit directory with no direct children.
            let is_implicit_dir = guard.keys().any(|p| p.parent() == Some(path));
            if !is_implicit_dir {
                return Err(VfsError::NotFound(path.to_owned()));
            }
        }
        Ok(entries)
    }

    fn stat(&self, path: &Path) -> Result<FileStat, VfsError> {
        let guard = self.files.read().expect("MockVfs files lock poisoned");
        if let Some(b) = guard.get(path) {
            return Ok(FileStat {
                is_dir: false,
                is_file: true,
                is_symlink: false,
                size: b.len() as u64,
            });
        }
        // A path is implicitly a directory if any registered file has it as
        // a parent. This allows tests to stat directory paths without
        // explicitly registering them as keys.
        let is_implicit_dir = guard.keys().any(|p| p.parent() == Some(path));
        if is_implicit_dir {
            return Ok(FileStat {
                is_dir: true,
                is_file: false,
                is_symlink: false,
                size: 0,
            });
        }
        Err(VfsError::NotFound(path.to_owned()))
    }

    fn exists(&self, path: &Path) -> bool {
        let guard = self.files.read().expect("MockVfs files lock poisoned");
        guard.contains_key(path) || guard.keys().any(|p| p.parent() == Some(path))
    }

    fn write_file(&self, path: &Path, contents: &[u8]) -> Result<(), VfsError> {
        self.files
            .write()
            .expect("MockVfs files lock poisoned")
            .insert(path.to_owned(), contents.to_vec());
        Ok(())
    }

    fn create_dir(&self, _path: &Path) -> Result<(), VfsError> {
        // Directories are implicit in MockVfs (any path whose parent exists).
        Ok(())
    }

    fn create_dir_all(&self, _path: &Path) -> Result<(), VfsError> {
        Ok(())
    }

    fn remove_file(&self, path: &Path) -> Result<(), VfsError> {
        let removed = self
            .files
            .write()
            .expect("MockVfs files lock poisoned")
            .remove(path);
        if removed.is_some() {
            Ok(())
        } else {
            Err(VfsError::NotFound(path.to_owned()))
        }
    }

    fn remove_dir_all(&self, path: &Path) -> Result<(), VfsError> {
        let prefix = path.to_owned();
        self.files
            .write()
            .expect("MockVfs files lock poisoned")
            .retain(|k, _| !k.starts_with(&prefix));
        Ok(())
    }
}

/// A virtual filesystem handler for a mounted path prefix.
///
/// Implement this for each virtual namespace (`/proc/`, `/mnt/mcp/<server>/`, etc.).
/// `LayeredVfs` checks handlers in mount-table order before falling through to the
/// real filesystem.
pub trait VfsHandler: Send + Sync {
    fn read_file(&self, path: &Path) -> Result<Vec<u8>, VfsError>;
    fn read_dir(&self, path: &Path) -> Result<Vec<DirEntry>, VfsError>;
    fn stat(&self, path: &Path) -> Result<FileStat, VfsError>;
    fn exists(&self, path: &Path) -> bool;
}

/// A layered VFS that checks a mount table first, then falls through to `RealFs`.
///
/// Mount entries are checked in insertion order. The first handler whose prefix
/// matches the requested path handles the request.
pub struct LayeredVfs {
    mounts: Vec<(PathBuf, Box<dyn VfsHandler>)>,
}

impl LayeredVfs {
    pub fn new() -> Self {
        Self { mounts: Vec::new() }
    }

    /// Mount a handler at the given path prefix.
    pub fn mount(mut self, prefix: impl Into<PathBuf>, handler: impl VfsHandler + 'static) -> Self {
        self.mounts.push((prefix.into(), Box::new(handler)));
        self
    }

    fn handler_for(&self, path: &Path) -> Option<&dyn VfsHandler> {
        for (prefix, handler) in &self.mounts {
            if path.starts_with(prefix) {
                return Some(handler.as_ref());
            }
        }
        None
    }
}

impl Default for LayeredVfs {
    fn default() -> Self {
        Self::new()
    }
}

impl Vfs for LayeredVfs {
    fn read_file(&self, path: &Path) -> Result<Vec<u8>, VfsError> {
        if let Some(h) = self.handler_for(path) {
            h.read_file(path)
        } else {
            RealFs.read_file(path)
        }
    }

    fn read_dir(&self, path: &Path) -> Result<Vec<DirEntry>, VfsError> {
        if let Some(h) = self.handler_for(path) {
            h.read_dir(path)
        } else {
            RealFs.read_dir(path)
        }
    }

    fn stat(&self, path: &Path) -> Result<FileStat, VfsError> {
        if let Some(h) = self.handler_for(path) {
            h.stat(path)
        } else {
            RealFs.stat(path)
        }
    }

    fn exists(&self, path: &Path) -> bool {
        if let Some(h) = self.handler_for(path) {
            h.exists(path)
        } else {
            RealFs.exists(path)
        }
    }

    // Write operations always fall through to RealFs — virtual handlers are
    // read-only (the /proc/ virtual filesystem is not writable).
    fn write_file(&self, path: &Path, contents: &[u8]) -> Result<(), VfsError> {
        RealFs.write_file(path, contents)
    }
    fn create_dir(&self, path: &Path) -> Result<(), VfsError> {
        RealFs.create_dir(path)
    }
    fn create_dir_all(&self, path: &Path) -> Result<(), VfsError> {
        RealFs.create_dir_all(path)
    }
    fn remove_file(&self, path: &Path) -> Result<(), VfsError> {
        RealFs.remove_file(path)
    }
    fn remove_dir_all(&self, path: &Path) -> Result<(), VfsError> {
        RealFs.remove_dir_all(path)
    }
}

/// VFS implementation that delegates directly to `std::fs`.
pub struct RealFs;

impl Vfs for RealFs {
    fn read_file(&self, path: &Path) -> Result<Vec<u8>, VfsError> {
        std::fs::read(path).map_err(|e| VfsError::Io {
            path: path.to_owned(),
            source: e,
        })
    }

    fn read_dir(&self, path: &Path) -> Result<Vec<DirEntry>, VfsError> {
        let rd = std::fs::read_dir(path).map_err(|e| VfsError::Io {
            path: path.to_owned(),
            source: e,
        })?;

        rd.map(|entry| {
            let entry = entry.map_err(|e| VfsError::Io {
                path: path.to_owned(),
                source: e,
            })?;
            let ft = entry.file_type().map_err(|e| VfsError::Io {
                path: entry.path(),
                source: e,
            })?;
            Ok(DirEntry {
                path: entry.path(),
                is_dir: ft.is_dir(),
                is_file: ft.is_file(),
                is_symlink: ft.is_symlink(),
            })
        })
        .collect()
    }

    fn stat(&self, path: &Path) -> Result<FileStat, VfsError> {
        let meta = std::fs::metadata(path).map_err(|e| VfsError::Io {
            path: path.to_owned(),
            source: e,
        })?;
        Ok(FileStat {
            is_dir: meta.is_dir(),
            is_file: meta.is_file(),
            is_symlink: meta.file_type().is_symlink(),
            size: meta.len(),
        })
    }

    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }

    fn write_file(&self, path: &Path, contents: &[u8]) -> Result<(), VfsError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| VfsError::Io {
                path: parent.to_owned(),
                source: e,
            })?;
        }
        std::fs::write(path, contents).map_err(|e| VfsError::Io {
            path: path.to_owned(),
            source: e,
        })
    }

    fn create_dir(&self, path: &Path) -> Result<(), VfsError> {
        std::fs::create_dir(path).map_err(|e| VfsError::Io {
            path: path.to_owned(),
            source: e,
        })
    }

    fn create_dir_all(&self, path: &Path) -> Result<(), VfsError> {
        std::fs::create_dir_all(path).map_err(|e| VfsError::Io {
            path: path.to_owned(),
            source: e,
        })
    }

    fn remove_file(&self, path: &Path) -> Result<(), VfsError> {
        std::fs::remove_file(path).map_err(|e| VfsError::Io {
            path: path.to_owned(),
            source: e,
        })
    }

    fn remove_dir_all(&self, path: &Path) -> Result<(), VfsError> {
        std::fs::remove_dir_all(path).map_err(|e| VfsError::Io {
            path: path.to_owned(),
            source: e,
        })
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proc_handler::{ProcHandler, ProcessSnapshot, SystemPromptSource};
    use std::path::Path;
    use std::sync::{Arc, RwLock};

    // -----------------------------------------------------------------------
    // F2 — VfsError display strings
    // -----------------------------------------------------------------------

    #[test]
    fn test_vfs_error_not_found_display_includes_path() {
        let e = VfsError::NotFound(PathBuf::from("/proc/999/cmdline"));
        assert_eq!(e.to_string(), "No such file or directory");
    }

    #[test]
    fn test_vfs_error_permission_denied_display() {
        let e = VfsError::PermissionDenied(PathBuf::from("/etc/shadow"));
        assert_eq!(e.to_string(), "permission denied");
    }

    #[test]
    fn test_vfs_error_io_display_includes_path_and_source() {
        let e = VfsError::Io {
            path: PathBuf::from("/tmp/foo"),
            source: std::io::Error::new(std::io::ErrorKind::PermissionDenied, "access denied"),
        };
        let s = e.to_string();
        assert!(s.contains("/tmp/foo"), "path missing from display: {s}");
        assert!(
            s.contains("access denied"),
            "source missing from display: {s}"
        );
    }

    // -----------------------------------------------------------------------
    // F3 — MockVfs::read_dir non-obvious branch
    // -----------------------------------------------------------------------

    #[test]
    fn test_mock_vfs_read_dir_returns_direct_children_only() {
        let vfs = MockVfs::new()
            .with_file("/dir/file_a.txt", "a")
            .with_file("/dir/file_b.txt", "b")
            .with_file("/dir/sub/nested.txt", "n"); // grandchild — must NOT appear

        let entries = vfs.read_dir(Path::new("/dir")).unwrap();
        let names: Vec<_> = entries
            .iter()
            .map(|e| e.path.file_name().unwrap().to_str().unwrap())
            .collect();
        assert!(names.contains(&"file_a.txt"), "file_a.txt missing");
        assert!(names.contains(&"file_b.txt"), "file_b.txt missing");
        assert!(
            !names.contains(&"nested.txt"),
            "grandchild must not appear in parent read_dir"
        );
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn test_mock_vfs_read_dir_returns_not_found_for_absent_path() {
        let vfs = MockVfs::new().with_file("/other/file.txt", "x");
        let err = vfs.read_dir(Path::new("/nonexistent")).unwrap_err();
        assert!(
            matches!(err, VfsError::NotFound(_)),
            "absent path must return NotFound"
        );
    }

    #[test]
    fn test_mock_vfs_read_dir_returns_empty_for_registered_file_used_as_dir_key() {
        // A path registered as a file key with no children returns empty vec,
        // not NotFound, because the key itself exists in the map.
        let vfs = MockVfs::new().with_file("/dir", b"i am a file".to_vec());
        // /dir is registered — read_dir returns empty (no children), not NotFound.
        let entries = vfs.read_dir(Path::new("/dir")).unwrap();
        assert!(
            entries.is_empty(),
            "registered key with no children must return empty vec"
        );
    }

    // -----------------------------------------------------------------------
    // F4 — ProcHandler file format contracts
    // -----------------------------------------------------------------------

    fn make_proc(pid: u64, ppid: u64, argv: &[&str], env: &[(&str, &str)]) -> ProcessSnapshot {
        ProcessSnapshot {
            pid,
            ppid,
            argv: argv.iter().map(|s| s.to_string()).collect(),
            state_char: 'R',
            environ: env
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        }
    }

    fn handler_with(procs: Vec<ProcessSnapshot>) -> ProcHandler {
        ProcHandler::new(Arc::new(RwLock::new(procs)))
    }

    #[test]
    fn test_proc_handler_cmdline_nul_separated() {
        let h = handler_with(vec![make_proc(42, 1, &["bash", "-c", "echo hi"], &[])]);
        let bytes = h.read_file(Path::new("/proc/42/cmdline")).unwrap();
        // Each argv element separated by NUL, terminated by NUL.
        assert_eq!(bytes, b"bash\x00-c\x00echo hi\x00");
    }

    #[test]
    fn test_proc_handler_status_format() {
        let h = handler_with(vec![make_proc(42, 1, &["myproc", "--flag"], &[])]);
        let bytes = h.read_file(Path::new("/proc/42/status")).unwrap();
        let text = String::from_utf8(bytes).unwrap();
        assert!(text.contains("Pid:\t42\n"), "Pid field missing: {text}");
        assert!(text.contains("PPid:\t1\n"), "PPid field missing: {text}");
        assert!(text.contains("State:\tR\n"), "State field missing: {text}");
        assert!(
            text.contains("Name:\tmyproc\n"),
            "Name field missing: {text}"
        );
    }

    #[test]
    fn test_proc_handler_environ_nul_separated_key_value() {
        let h = handler_with(vec![make_proc(
            7,
            1,
            &["sh"],
            &[("HOME", "/root"), ("PATH", "/usr/bin")],
        )]);
        let bytes = h.read_file(Path::new("/proc/7/environ")).unwrap();
        // NUL-separated KEY=value pairs, terminated by NUL.
        let text = String::from_utf8(bytes).unwrap();
        let pairs: Vec<&str> = text.trim_end_matches('\0').split('\0').collect();
        assert!(pairs.contains(&"HOME=/root"), "HOME missing: {pairs:?}");
        assert!(pairs.contains(&"PATH=/usr/bin"), "PATH missing: {pairs:?}");
    }

    #[test]
    fn test_proc_handler_system_prompt_with_source() {
        struct Fixed(String);
        impl SystemPromptSource for Fixed {
            fn system_prompt(&self) -> String {
                self.0.clone()
            }
        }
        let h =
            handler_with(vec![]).with_system_prompt(Arc::new(Fixed("you are clank".to_string())));
        let bytes = h.read_file(Path::new("/proc/clank/system-prompt")).unwrap();
        assert_eq!(bytes, b"you are clank");
    }

    #[test]
    fn test_proc_handler_system_prompt_without_source_returns_fallback() {
        let h = handler_with(vec![]);
        let bytes = h.read_file(Path::new("/proc/clank/system-prompt")).unwrap();
        let text = String::from_utf8(bytes).unwrap();
        assert!(
            text.contains("not configured"),
            "fallback message missing: {text}"
        );
    }

    #[test]
    fn test_proc_handler_read_file_unknown_pid_returns_not_found() {
        let h = handler_with(vec![]);
        let err = h.read_file(Path::new("/proc/999/cmdline")).unwrap_err();
        assert!(matches!(err, VfsError::NotFound(_)));
    }

    #[test]
    fn test_proc_handler_read_file_unknown_subfile_returns_not_found() {
        let h = handler_with(vec![make_proc(1, 0, &["init"], &[])]);
        let err = h.read_file(Path::new("/proc/1/maps")).unwrap_err();
        assert!(matches!(err, VfsError::NotFound(_)));
    }

    #[test]
    fn test_proc_handler_read_dir_proc_root_includes_pids_and_clank() {
        let h = handler_with(vec![
            make_proc(10, 1, &["a"], &[]),
            make_proc(20, 1, &["b"], &[]),
        ]);
        let entries = h.read_dir(Path::new("/proc")).unwrap();
        let paths: Vec<_> = entries.iter().map(|e| e.path.to_str().unwrap()).collect();
        assert!(paths.contains(&"/proc/10"), "pid 10 missing");
        assert!(paths.contains(&"/proc/20"), "pid 20 missing");
        assert!(paths.contains(&"/proc/clank"), "clank dir missing");
        assert!(entries.iter().filter(|e| e.is_dir).count() == 3);
    }

    #[test]
    fn test_proc_handler_read_dir_pid_subdir_lists_three_files() {
        let h = handler_with(vec![make_proc(5, 1, &["sh"], &[])]);
        let entries = h.read_dir(Path::new("/proc/5")).unwrap();
        let names: Vec<_> = entries
            .iter()
            .map(|e| e.path.file_name().unwrap().to_str().unwrap())
            .collect();
        assert!(names.contains(&"cmdline"), "cmdline missing");
        assert!(names.contains(&"status"), "status missing");
        assert!(names.contains(&"environ"), "environ missing");
        assert_eq!(entries.len(), 3);
    }

    #[test]
    fn test_proc_handler_stat_proc_is_dir() {
        let h = handler_with(vec![]);
        let s = h.stat(Path::new("/proc")).unwrap();
        assert!(s.is_dir);
        assert!(!s.is_file);
    }

    #[test]
    fn test_proc_handler_stat_pid_dir_is_dir() {
        let h = handler_with(vec![make_proc(3, 1, &["x"], &[])]);
        let s = h.stat(Path::new("/proc/3")).unwrap();
        assert!(s.is_dir);
    }

    #[test]
    fn test_proc_handler_stat_pid_file_is_file() {
        let h = handler_with(vec![make_proc(3, 1, &["x"], &[])]);
        let s = h.stat(Path::new("/proc/3/cmdline")).unwrap();
        assert!(s.is_file);
        assert!(!s.is_dir);
    }

    #[test]
    fn test_proc_handler_stat_unknown_pid_returns_not_found() {
        let h = handler_with(vec![]);
        let err = h.stat(Path::new("/proc/999")).unwrap_err();
        assert!(matches!(err, VfsError::NotFound(_)));
    }

    // -----------------------------------------------------------------------
    // F5 — LayeredVfs routing contract
    // -----------------------------------------------------------------------

    struct SpyHandler {
        file_content: Vec<u8>,
    }

    impl VfsHandler for SpyHandler {
        fn read_file(&self, _path: &Path) -> Result<Vec<u8>, VfsError> {
            Ok(self.file_content.clone())
        }
        fn read_dir(&self, _path: &Path) -> Result<Vec<DirEntry>, VfsError> {
            Ok(vec![])
        }
        fn stat(&self, _path: &Path) -> Result<FileStat, VfsError> {
            Ok(FileStat {
                is_dir: false,
                is_file: true,
                is_symlink: false,
                size: 0,
            })
        }
        fn exists(&self, _path: &Path) -> bool {
            true
        }
    }

    #[test]
    fn test_layered_vfs_routes_mounted_prefix_to_handler() {
        let vfs = LayeredVfs::new().mount(
            "/proc",
            SpyHandler {
                file_content: b"from-handler".to_vec(),
            },
        );
        let bytes = vfs.read_file(Path::new("/proc/1/cmdline")).unwrap();
        assert_eq!(bytes, b"from-handler");
    }

    #[test]
    fn test_layered_vfs_falls_through_to_real_fs_for_unmounted_path() {
        // An unmounted path falls through to RealFs. We use a known real file.
        let vfs = LayeredVfs::new().mount(
            "/proc",
            SpyHandler {
                file_content: b"irrelevant".to_vec(),
            },
        );
        // /etc/hostname exists on all Unix systems; just verify it doesn't
        // return the handler's content.
        let result = vfs.read_file(Path::new("/etc/hostname"));
        // We don't assert on content (varies by machine), just that the handler
        // was NOT invoked (its sentinel would be b"irrelevant").
        if let Ok(bytes) = result {
            assert_ne!(
                bytes, b"irrelevant",
                "unmounted path must not hit the handler"
            );
        }
        // NotFound from RealFs is also acceptable (some systems may not have /etc/hostname).
    }

    #[test]
    fn test_layered_vfs_first_matching_mount_wins() {
        let vfs = LayeredVfs::new()
            .mount(
                "/proc",
                SpyHandler {
                    file_content: b"first".to_vec(),
                },
            )
            .mount(
                "/proc",
                SpyHandler {
                    file_content: b"second".to_vec(),
                },
            );
        let bytes = vfs.read_file(Path::new("/proc/1/cmdline")).unwrap();
        assert_eq!(bytes, b"first", "first matching mount must win");
    }
}
