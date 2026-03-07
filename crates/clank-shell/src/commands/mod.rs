pub mod cat;
pub mod env_cmd;
pub mod export;
pub mod grep;
pub mod ls;
pub mod mkdir;
pub mod prompt_user;
pub mod ps;
pub mod rm;
pub mod stat_cmd;
pub mod touch;

use std::path::{Path, PathBuf};

/// Resolve a path argument against the shell's current working directory.
///
/// Absolute paths are returned unchanged. Relative paths are joined to `cwd`.
/// No canonicalisation is performed — symlinks and `..` components are
/// preserved. This matches the behaviour of standard Unix tools, which resolve
/// relative paths against the process cwd without canonicalising.
///
/// Always use this instead of passing a raw path string to `Vfs` methods.
/// `std::env::current_dir()` is the OS process cwd — it is NOT updated when
/// the user runs `cd` — and must never be used for path resolution in commands.
pub(crate) fn resolve(cwd: &Path, path: &str) -> PathBuf {
    let p = Path::new(path);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        cwd.join(p)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_relative_joins_to_cwd() {
        let cwd = Path::new("/tmp");
        assert_eq!(resolve(cwd, "demo"), PathBuf::from("/tmp/demo"));
    }

    #[test]
    fn test_resolve_absolute_passes_through() {
        let cwd = Path::new("/tmp");
        assert_eq!(
            resolve(cwd, "/absolute/path"),
            PathBuf::from("/absolute/path")
        );
    }

    #[test]
    fn test_resolve_dotdot_not_canonicalised() {
        let cwd = Path::new("/tmp");
        // `..` is preserved — callers that need canonicalisation must do it
        // themselves. This matches standard Unix tool behaviour.
        assert_eq!(resolve(cwd, "../sibling"), PathBuf::from("/tmp/../sibling"));
    }

    #[test]
    fn test_resolve_dot_joins_to_cwd() {
        let cwd = Path::new("/a/b/c");
        assert_eq!(resolve(cwd, "."), PathBuf::from("/a/b/c/."));
    }
}
