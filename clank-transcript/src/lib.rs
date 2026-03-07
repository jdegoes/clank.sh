//! clank-transcript — the shell's sliding-window transcript.
//!
//! Owns the [`Transcript`] type and the process-global accessor [`global`].
//! Both `clank-core` (recording) and `clank-builtins` (`context` builtin)
//! depend on this crate to avoid a circular dependency between those two.
//!
//! ## Design constraint
//!
//! The process-global is the only way to bridge the gap between the
//! `clank-core` recording call site and the `clank-builtins` `context`
//! builtin, because `brush-core`'s builtin registration API uses bare `fn`
//! pointers with no user-data slot. This is correct for the current
//! single-shell-per-process model. If clank ever runs multiple shells in one
//! process this will need revisiting.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex, OnceLock};

/// Default maximum number of entries the sliding window holds before the
/// oldest entry is evicted on each new [`Transcript::push`].
pub const DEFAULT_MAX_ENTRIES: usize = 1000;

/// A bounded sliding window of shell transcript entries.
///
/// Each entry is a string appended at a `run_string` call site — typically
/// the command text typed or executed. When the window reaches `max_entries`
/// capacity, the oldest entry is silently dropped to make room.
pub struct Transcript {
    entries: VecDeque<String>,
    max_entries: usize,
}

impl Transcript {
    /// Create a new transcript with the given entry capacity.
    pub fn new(max_entries: usize) -> Self {
        Self {
            entries: VecDeque::new(),
            max_entries,
        }
    }

    /// Append an entry. If the window is at capacity, the oldest entry is
    /// dropped first.
    pub fn push(&mut self, entry: impl Into<String>) {
        if self.entries.len() == self.max_entries {
            self.entries.pop_front();
        }
        self.entries.push_back(entry.into());
    }

    /// Discard all entries.
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Drop the oldest `n` entries. If `n` is zero, this is a no-op. If `n`
    /// meets or exceeds the current length, all entries are dropped.
    pub fn trim(&mut self, n: usize) {
        let to_drop = n.min(self.entries.len());
        self.entries.drain(..to_drop);
    }

    /// Iterate over all entries in order from oldest to newest.
    pub fn entries(&self) -> impl Iterator<Item = &str> {
        self.entries.iter().map(String::as_str)
    }

    /// Number of entries currently in the window.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns `true` if the window contains no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

// ---------------------------------------------------------------------------
// Process-global accessor
// ---------------------------------------------------------------------------

static GLOBAL: OnceLock<Arc<Mutex<Transcript>>> = OnceLock::new();

/// Return a clone of the process-global [`Arc`] wrapping the shared
/// [`Transcript`].
///
/// The transcript is initialized with [`DEFAULT_MAX_ENTRIES`] on first call.
/// Subsequent calls return a clone of the same `Arc`.
pub fn global() -> Arc<Mutex<Transcript>> {
    GLOBAL
        .get_or_init(|| Arc::new(Mutex::new(Transcript::new(DEFAULT_MAX_ENTRIES))))
        .clone()
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // --- push / entries ---

    #[test]
    fn push_appends_in_order() {
        let mut t = Transcript::new(10);
        t.push("a");
        t.push("b");
        t.push("c");
        let got: Vec<&str> = t.entries().collect();
        assert_eq!(got, vec!["a", "b", "c"]);
    }

    #[test]
    fn new_transcript_is_empty() {
        let t = Transcript::new(10);
        assert!(t.is_empty());
        assert_eq!(t.len(), 0);
    }

    // --- sliding-window eviction ---

    #[test]
    fn push_evicts_oldest_at_capacity() {
        let mut t = Transcript::new(3);
        t.push("a");
        t.push("b");
        t.push("c");
        t.push("d"); // "a" should be evicted
        let got: Vec<&str> = t.entries().collect();
        assert_eq!(got, vec!["b", "c", "d"]);
    }

    #[test]
    fn push_to_capacity_does_not_evict() {
        let mut t = Transcript::new(3);
        t.push("a");
        t.push("b");
        t.push("c");
        assert_eq!(t.len(), 3);
        let got: Vec<&str> = t.entries().collect();
        assert_eq!(got, vec!["a", "b", "c"]);
    }

    #[test]
    fn repeated_push_beyond_capacity_keeps_newest() {
        let mut t = Transcript::new(2);
        for i in 0..10usize {
            t.push(i.to_string());
        }
        assert_eq!(t.len(), 2);
        let got: Vec<&str> = t.entries().collect();
        assert_eq!(got, vec!["8", "9"]);
    }

    // --- clear ---

    #[test]
    fn clear_removes_all_entries() {
        let mut t = Transcript::new(10);
        t.push("a");
        t.push("b");
        t.clear();
        assert!(t.is_empty());
    }

    #[test]
    fn clear_allows_subsequent_push() {
        let mut t = Transcript::new(10);
        t.push("a");
        t.clear();
        t.push("b");
        let got: Vec<&str> = t.entries().collect();
        assert_eq!(got, vec!["b"]);
    }

    // --- trim ---

    #[test]
    fn trim_zero_is_noop() {
        let mut t = Transcript::new(10);
        t.push("a");
        t.push("b");
        t.trim(0);
        let got: Vec<&str> = t.entries().collect();
        assert_eq!(got, vec!["a", "b"]);
    }

    #[test]
    fn trim_drops_oldest_n() {
        let mut t = Transcript::new(10);
        t.push("a");
        t.push("b");
        t.push("c");
        t.trim(2);
        let got: Vec<&str> = t.entries().collect();
        assert_eq!(got, vec!["c"]);
    }

    #[test]
    fn trim_exact_len_clears_all() {
        let mut t = Transcript::new(10);
        t.push("a");
        t.push("b");
        t.trim(2);
        assert!(t.is_empty());
    }

    #[test]
    fn trim_exceeding_len_clears_all_without_error() {
        let mut t = Transcript::new(10);
        t.push("a");
        t.push("b");
        t.trim(999);
        assert!(t.is_empty());
    }
}
