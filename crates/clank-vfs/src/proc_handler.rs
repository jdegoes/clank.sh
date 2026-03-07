use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use crate::{DirEntry, FileStat, VfsError, VfsHandler};

/// Process table snapshot entry — supplied by clank-shell at mount time.
/// Defined here to avoid a circular crate dependency.
#[derive(Debug, Clone)]
pub struct ProcessSnapshot {
    pub pid: u64,
    pub ppid: u64,
    pub argv: Vec<String>,
    pub state_char: char,
    pub environ: Vec<(String, String)>,
}

/// Source of the current system prompt (assembled from manifest registry).
pub trait SystemPromptSource: Send + Sync {
    fn system_prompt(&self) -> String;
}

/// A `VfsHandler` that serves the `/proc/` virtual namespace.
///
/// Provides:
///   /proc/                       — directory of PID subdirectories
///   /proc/<pid>/cmdline          — argv joined by NUL
///   /proc/<pid>/status           — text status file
///   /proc/<pid>/environ          — environment as NUL-separated KEY=value pairs
///   /proc/clank/                 — clank-specific subtree
///   /proc/clank/system-prompt    — current system prompt, computed on read
pub struct ProcHandler {
    process_source: Arc<RwLock<Vec<ProcessSnapshot>>>,
    system_prompt_source: Option<Arc<dyn SystemPromptSource>>,
}

impl ProcHandler {
    pub fn new(process_source: Arc<RwLock<Vec<ProcessSnapshot>>>) -> Self {
        Self {
            process_source,
            system_prompt_source: None,
        }
    }

    pub fn with_system_prompt(mut self, source: Arc<dyn SystemPromptSource>) -> Self {
        self.system_prompt_source = Some(source);
        self
    }

    fn find_proc(&self, pid: u64) -> Option<ProcessSnapshot> {
        self.process_source
            .read()
            .expect("process source lock poisoned")
            .iter()
            .find(|p| p.pid == pid)
            .cloned()
    }
}

impl VfsHandler for ProcHandler {
    fn read_file(&self, path: &Path) -> Result<Vec<u8>, VfsError> {
        let components: Vec<_> = path.components().collect();

        // /proc/clank/system-prompt
        if path == Path::new("/proc/clank/system-prompt") {
            let content = self
                .system_prompt_source
                .as_ref()
                .map(|s| s.system_prompt())
                .unwrap_or_else(|| "(system prompt not configured)\n".to_string());
            return Ok(content.into_bytes());
        }

        // /proc/<pid>/<file>
        if components.len() == 4 {
            // components: [RootDir, "proc", "<pid>", "<file>"]
            let pid_str = components[2].as_os_str().to_string_lossy();
            let filename = components[3].as_os_str().to_string_lossy();

            if let Ok(pid) = pid_str.parse::<u64>() {
                if let Some(proc) = self.find_proc(pid) {
                    return match filename.as_ref() {
                        "cmdline" => {
                            let content = proc.argv.join("\0") + "\0";
                            Ok(content.into_bytes())
                        }
                        "status" => {
                            let content = format!(
                                "Pid:\t{}\nPPid:\t{}\nState:\t{}\nName:\t{}\n",
                                proc.pid,
                                proc.ppid,
                                proc.state_char,
                                proc.argv.first().map(String::as_str).unwrap_or("")
                            );
                            Ok(content.into_bytes())
                        }
                        "environ" => {
                            let content = proc
                                .environ
                                .iter()
                                .map(|(k, v)| format!("{k}={v}"))
                                .collect::<Vec<_>>()
                                .join("\0")
                                + "\0";
                            Ok(content.into_bytes())
                        }
                        _ => Err(VfsError::NotFound(path.to_owned())),
                    };
                }
            }
        }

        Err(VfsError::NotFound(path.to_owned()))
    }

    fn read_dir(&self, path: &Path) -> Result<Vec<DirEntry>, VfsError> {
        // /proc/ — list PID directories + clank/
        if path == Path::new("/proc") || path == Path::new("/proc/") {
            let mut entries: Vec<DirEntry> = self
                .process_source
                .read()
                .expect("process source lock poisoned")
                .iter()
                .map(|p| DirEntry {
                    path: PathBuf::from(format!("/proc/{}", p.pid)),
                    is_dir: true,
                    is_file: false,
                    is_symlink: false,
                })
                .collect();
            entries.push(DirEntry {
                path: PathBuf::from("/proc/clank"),
                is_dir: true,
                is_file: false,
                is_symlink: false,
            });
            return Ok(entries);
        }

        // /proc/<pid>/ — list per-process files
        let components: Vec<_> = path.components().collect();
        if components.len() == 3 {
            let pid_str = components[2].as_os_str().to_string_lossy();
            if let Ok(pid) = pid_str.parse::<u64>() {
                if self.find_proc(pid).is_some() {
                    let base = format!("/proc/{pid}");
                    return Ok(vec![
                        DirEntry {
                            path: PathBuf::from(format!("{base}/cmdline")),
                            is_dir: false,
                            is_file: true,
                            is_symlink: false,
                        },
                        DirEntry {
                            path: PathBuf::from(format!("{base}/status")),
                            is_dir: false,
                            is_file: true,
                            is_symlink: false,
                        },
                        DirEntry {
                            path: PathBuf::from(format!("{base}/environ")),
                            is_dir: false,
                            is_file: true,
                            is_symlink: false,
                        },
                    ]);
                }
            }
        }

        // /proc/clank/
        if path == Path::new("/proc/clank") {
            return Ok(vec![DirEntry {
                path: PathBuf::from("/proc/clank/system-prompt"),
                is_dir: false,
                is_file: true,
                is_symlink: false,
            }]);
        }

        Err(VfsError::NotFound(path.to_owned()))
    }

    fn stat(&self, path: &Path) -> Result<FileStat, VfsError> {
        if path == Path::new("/proc") {
            return Ok(FileStat {
                is_dir: true,
                is_file: false,
                is_symlink: false,
                size: 0,
            });
        }
        if path == Path::new("/proc/clank") {
            return Ok(FileStat {
                is_dir: true,
                is_file: false,
                is_symlink: false,
                size: 0,
            });
        }
        if path == Path::new("/proc/clank/system-prompt") {
            let size = self
                .system_prompt_source
                .as_ref()
                .map(|s| s.system_prompt().len() as u64)
                .unwrap_or(0);
            return Ok(FileStat {
                is_dir: false,
                is_file: true,
                is_symlink: false,
                size,
            });
        }

        // /proc/<pid>
        let components: Vec<_> = path.components().collect();
        if components.len() == 3 {
            let pid_str = components[2].as_os_str().to_string_lossy();
            if let Ok(pid) = pid_str.parse::<u64>() {
                if self.find_proc(pid).is_some() {
                    return Ok(FileStat {
                        is_dir: true,
                        is_file: false,
                        is_symlink: false,
                        size: 0,
                    });
                }
            }
        }

        // /proc/<pid>/<file>
        if components.len() == 4 {
            let pid_str = components[2].as_os_str().to_string_lossy();
            let filename = components[3].as_os_str().to_string_lossy();
            if let Ok(pid) = pid_str.parse::<u64>() {
                if self.find_proc(pid).is_some() {
                    match filename.as_ref() {
                        "cmdline" | "status" | "environ" => {
                            return Ok(FileStat {
                                is_dir: false,
                                is_file: true,
                                is_symlink: false,
                                size: 0,
                            });
                        }
                        _ => {}
                    }
                }
            }
        }

        Err(VfsError::NotFound(path.to_owned()))
    }

    fn exists(&self, path: &Path) -> bool {
        self.stat(path).is_ok()
    }
}
