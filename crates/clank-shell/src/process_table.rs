use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, RwLock};
use std::time::SystemTime;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// The type of a synthetic process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProcessType {
    /// `parent-shell` scoped command (cd, export, etc.)
    ParentShell,
    /// `shell-internal` scoped command (context, jobs, etc.)
    ShellInternal,
    /// `subprocess` scoped command (ls, ask, scripts, etc.)
    Subprocess,
}

/// The status of a synthetic process, matching the spec's state table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProcessStatus {
    /// Running / active.
    Running,
    /// Sleeping / waiting on remote work.
    Sleeping,
    /// Suspended (e.g. Ctrl-Z, Phase 5).
    Suspended,
    /// Completed, not yet reaped.
    Zombie { exit_code: i32 },
    /// Paused — awaiting user authorization or `prompt-user` input.
    Paused,
}

impl ProcessStatus {
    /// The single-character state code shown in `ps` output.
    pub fn state_char(&self) -> char {
        match self {
            ProcessStatus::Running => 'R',
            ProcessStatus::Sleeping => 'S',
            ProcessStatus::Suspended => 'T',
            ProcessStatus::Zombie { .. } => 'Z',
            ProcessStatus::Paused => 'P',
        }
    }
}

/// A single entry in the process table.
#[derive(Debug)]
pub struct ProcessEntry {
    pub pid: u64,
    pub ppid: u64,
    pub shell_id: u64,
    pub process_type: ProcessType,
    /// Full argument vector including argv[0].
    pub argv: Vec<String>,
    pub status: ProcessStatus,
    pub start_time: SystemTime,
    /// Handle for aborting backgrounded tasks. `None` for foreground processes.
    pub abort_handle: Option<tokio::task::AbortHandle>,
}

// ---------------------------------------------------------------------------
// Global process table
// ---------------------------------------------------------------------------

/// Key: (shell_id, pid)
type TableKey = (u64, u64);

static PROCESS_TABLE: LazyLock<RwLock<HashMap<TableKey, ProcessEntry>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

/// Per-shell PID counters. Each shell gets its own monotonically increasing PID.
static PID_COUNTERS: LazyLock<RwLock<HashMap<u64, AtomicU64>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

fn next_pid(shell_id: u64) -> u64 {
    // Fast path: counter already exists.
    {
        let guard = PID_COUNTERS.read().expect("pid counters poisoned");
        if let Some(counter) = guard.get(&shell_id) {
            return counter.fetch_add(1, Ordering::Relaxed);
        }
    }
    // Slow path: initialise counter for this shell.
    let mut guard = PID_COUNTERS.write().expect("pid counters poisoned");
    guard
        .entry(shell_id)
        .or_insert_with(|| AtomicU64::new(1))
        .fetch_add(1, Ordering::Relaxed)
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Register a new process in the table. Returns the assigned PID.
pub fn spawn(shell_id: u64, ppid: u64, argv: Vec<String>, process_type: ProcessType) -> u64 {
    let pid = next_pid(shell_id);
    let entry = ProcessEntry {
        pid,
        ppid,
        shell_id,
        process_type,
        argv,
        status: ProcessStatus::Running,
        start_time: SystemTime::now(),
        abort_handle: None,
    };
    PROCESS_TABLE
        .write()
        .expect("process table poisoned")
        .insert((shell_id, pid), entry);
    pid
}

/// Mark a process as complete (Zombie). Called when `Process::run()` returns.
pub fn complete(shell_id: u64, pid: u64, exit_code: i32) {
    let mut guard = PROCESS_TABLE.write().expect("process table poisoned");
    if let Some(entry) = guard.get_mut(&(shell_id, pid)) {
        entry.status = ProcessStatus::Zombie { exit_code };
        entry.abort_handle = None;
    }
}

/// Remove a completed process from the table (reap).
pub fn reap(shell_id: u64, pid: u64) {
    PROCESS_TABLE
        .write()
        .expect("process table poisoned")
        .remove(&(shell_id, pid));
}

/// Update a process's status.
pub fn set_status(shell_id: u64, pid: u64, status: ProcessStatus) {
    let mut guard = PROCESS_TABLE.write().expect("process table poisoned");
    if let Some(entry) = guard.get_mut(&(shell_id, pid)) {
        entry.status = status;
    }
}

/// Attach an abort handle to a backgrounded process.
pub fn set_abort_handle(shell_id: u64, pid: u64, handle: tokio::task::AbortHandle) {
    let mut guard = PROCESS_TABLE.write().expect("process table poisoned");
    if let Some(entry) = guard.get_mut(&(shell_id, pid)) {
        entry.abort_handle = Some(handle);
    }
}

/// Abort a running process by PID. Returns true if the process was found and aborted.
pub fn kill(shell_id: u64, pid: u64) -> bool {
    let mut guard = PROCESS_TABLE.write().expect("process table poisoned");
    if let Some(entry) = guard.get_mut(&(shell_id, pid)) {
        if let Some(handle) = entry.abort_handle.take() {
            handle.abort();
            entry.status = ProcessStatus::Zombie { exit_code: 130 };
            return true;
        }
    }
    false
}

/// Return a snapshot of all process entries for a given shell.
pub fn snapshot(shell_id: u64) -> Vec<ProcessEntry> {
    let guard = PROCESS_TABLE.read().expect("process table poisoned");
    guard
        .iter()
        .filter(|((sid, _), _)| *sid == shell_id)
        .map(|(_, entry)| ProcessEntry {
            pid: entry.pid,
            ppid: entry.ppid,
            shell_id: entry.shell_id,
            process_type: entry.process_type.clone(),
            argv: entry.argv.clone(),
            status: entry.status.clone(),
            start_time: entry.start_time,
            abort_handle: None, // not cloneable; omit from snapshot
        })
        .collect()
}

/// Return a single process entry by PID.
pub fn get(shell_id: u64, pid: u64) -> Option<ProcessEntry> {
    let guard = PROCESS_TABLE.read().expect("process table poisoned");
    guard.get(&(shell_id, pid)).map(|e| ProcessEntry {
        pid: e.pid,
        ppid: e.ppid,
        shell_id: e.shell_id,
        process_type: e.process_type.clone(),
        argv: e.argv.clone(),
        status: e.status.clone(),
        start_time: e.start_time,
        abort_handle: None,
    })
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn unique_shell_id() -> u64 {
        static COUNTER: AtomicU64 = AtomicU64::new(10_000);
        COUNTER.fetch_add(1, Ordering::Relaxed)
    }

    #[test]
    fn test_process_table_spawn_assigns_unique_increasing_pids() {
        let sid = unique_shell_id();
        let pid1 = spawn(sid, 0, vec!["ls".into()], ProcessType::Subprocess);
        let pid2 = spawn(sid, 0, vec!["cat".into()], ProcessType::Subprocess);
        assert!(pid2 > pid1, "PIDs must be monotonically increasing");
        reap(sid, pid1);
        reap(sid, pid2);
    }

    #[test]
    fn test_process_table_complete_marks_zombie() {
        let sid = unique_shell_id();
        let pid = spawn(sid, 0, vec!["ask".into()], ProcessType::Subprocess);
        complete(sid, pid, 0);
        let entry = get(sid, pid).expect("entry missing after complete");
        assert_eq!(entry.status, ProcessStatus::Zombie { exit_code: 0 });
        reap(sid, pid);
    }

    #[test]
    fn test_process_table_reap_removes_entry() {
        let sid = unique_shell_id();
        let pid = spawn(sid, 0, vec!["ls".into()], ProcessType::Subprocess);
        complete(sid, pid, 0);
        reap(sid, pid);
        assert!(get(sid, pid).is_none(), "entry should be gone after reap");
    }

    #[test]
    fn test_process_table_status_transitions() {
        let sid = unique_shell_id();
        let pid = spawn(
            sid,
            0,
            vec!["prompt-user".into()],
            ProcessType::ShellInternal,
        );

        set_status(sid, pid, ProcessStatus::Paused);
        assert_eq!(get(sid, pid).unwrap().status, ProcessStatus::Paused);

        set_status(sid, pid, ProcessStatus::Running);
        assert_eq!(get(sid, pid).unwrap().status, ProcessStatus::Running);

        complete(sid, pid, 0);
        assert_eq!(
            get(sid, pid).unwrap().status,
            ProcessStatus::Zombie { exit_code: 0 }
        );
        reap(sid, pid);
    }

    #[test]
    fn test_process_table_snapshot_filters_by_shell() {
        let sid1 = unique_shell_id();
        let sid2 = unique_shell_id();

        let pid1 = spawn(sid1, 0, vec!["ls".into()], ProcessType::Subprocess);
        let pid2 = spawn(sid2, 0, vec!["cat".into()], ProcessType::Subprocess);

        let snap1 = snapshot(sid1);
        let snap2 = snapshot(sid2);

        // snap1 must contain pid1 (belonging to sid1)
        assert!(snap1.iter().any(|e| e.pid == pid1 && e.shell_id == sid1));
        // snap1 must NOT contain any entry with shell_id == sid2
        assert!(!snap1.iter().any(|e| e.shell_id == sid2));
        // snap2 must contain pid2 (belonging to sid2)
        assert!(snap2.iter().any(|e| e.pid == pid2 && e.shell_id == sid2));
        // snap2 must NOT contain any entry with shell_id == sid1
        assert!(!snap2.iter().any(|e| e.shell_id == sid1));

        reap(sid1, pid1);
        reap(sid2, pid2);
    }

    #[test]
    fn test_state_chars() {
        assert_eq!(ProcessStatus::Running.state_char(), 'R');
        assert_eq!(ProcessStatus::Sleeping.state_char(), 'S');
        assert_eq!(ProcessStatus::Suspended.state_char(), 'T');
        assert_eq!(ProcessStatus::Zombie { exit_code: 0 }.state_char(), 'Z');
        assert_eq!(ProcessStatus::Paused.state_char(), 'P');
    }
}
