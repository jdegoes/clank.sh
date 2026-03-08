//! clank-transcript — the shell's sliding-window transcript.
//!
//! Owns the [`Transcript`], [`TranscriptEntry`], and [`EntryKind`] types, and
//! the process-global accessor [`global`].
//!
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

use chrono::{DateTime, SecondsFormat, Utc};

pub mod redactor;
pub use redactor::Redactor;

/// Default maximum number of entries the sliding window holds before the
/// oldest entry is evicted on each new [`Transcript::push`].
pub const DEFAULT_MAX_ENTRIES: usize = 1000;

// ---------------------------------------------------------------------------
// Entry types
// ---------------------------------------------------------------------------

/// The kind of content a [`TranscriptEntry`] represents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntryKind {
    /// A command typed by the user or executed by the shell.
    Command(String),
    /// Captured stdout from a command.
    Output(String),
    /// A response from an AI model via `ask`. Not yet produced by any
    /// current code path; present for type completeness.
    AiResponse(String),
}

impl EntryKind {
    /// The lowercase string tag used when formatting the entry for display.
    pub fn tag(&self) -> &'static str {
        match self {
            EntryKind::Command(_) => "command",
            EntryKind::Output(_) => "output",
            EntryKind::AiResponse(_) => "ai_response",
        }
    }

    /// The text content of the entry.
    pub fn text(&self) -> &str {
        match self {
            EntryKind::Command(s) | EntryKind::Output(s) | EntryKind::AiResponse(s) => s,
        }
    }
}

/// A single entry in the shell transcript, with timestamp and kind.
#[derive(Debug, Clone)]
pub struct TranscriptEntry {
    /// When the entry was recorded, in UTC.
    pub timestamp: DateTime<Utc>,
    /// The kind and content of the entry.
    pub kind: EntryKind,
}

impl TranscriptEntry {
    fn new(kind: EntryKind) -> Self {
        Self {
            timestamp: Utc::now(),
            kind,
        }
    }

    /// Create a `Command` entry timestamped now.
    pub fn command(text: impl Into<String>) -> Self {
        Self::new(EntryKind::Command(text.into()))
    }

    /// Create an `Output` entry timestamped now.
    pub fn output(text: impl Into<String>) -> Self {
        Self::new(EntryKind::Output(text.into()))
    }

    /// Create an `AiResponse` entry timestamped now.
    pub fn ai_response(text: impl Into<String>) -> Self {
        Self::new(EntryKind::AiResponse(text.into()))
    }

    /// Format the entry without a timestamp.
    ///
    /// Format: `<kind>: <text>`
    ///
    /// This is the default output of `context show`. Timestamps are omitted
    /// because they are implementation detail noise for most consumers —
    /// testing, scripting, and human inspection all benefit from the simpler
    /// form. Use [`display_with_timestamps`] to include them.
    pub fn display_plain(&self) -> String {
        format!("{}: {}", self.kind.tag(), self.kind.text())
    }

    /// Format the entry with a full RFC 3339 timestamp prefix.
    ///
    /// Format: `[<rfc3339-secs>] <kind>: <text>`
    ///
    /// Opt-in via `context show --timestamps`.
    pub fn display_with_timestamps(&self) -> String {
        let ts = self.timestamp.to_rfc3339_opts(SecondsFormat::Secs, true);
        format!("[{}] {}: {}", ts, self.kind.tag(), self.kind.text())
    }
}

// ---------------------------------------------------------------------------
// Transcript
// ---------------------------------------------------------------------------

/// A bounded sliding window of shell transcript entries.
///
/// When the window reaches `max_entries` capacity, the oldest entry is
/// silently evicted to make room for each new [`Transcript::push`].
///
/// Every entry is passed through the owned [`Redactor`] before storage.
/// Use [`Transcript::with_redactor`] to supply a custom redactor (e.g.
/// [`Redactor::none`] in tests to avoid false positives on synthetic data).
pub struct Transcript {
    entries: VecDeque<TranscriptEntry>,
    max_entries: usize,
    redactor: Redactor,
}

impl Transcript {
    /// Create a new transcript with [`Redactor::default`] and the given
    /// entry capacity.
    pub fn new(max_entries: usize) -> Self {
        Self::with_redactor(max_entries, Redactor::default())
    }

    /// Create a new transcript with an explicit [`Redactor`].
    pub fn with_redactor(max_entries: usize, redactor: Redactor) -> Self {
        Self {
            entries: VecDeque::new(),
            max_entries,
            redactor,
        }
    }

    /// Append an entry after scrubbing its text through the redactor.
    ///
    /// If the window is at capacity, the oldest entry is dropped first.
    pub fn push(&mut self, entry: TranscriptEntry) {
        let entry = self.redactor.scrub_entry(entry);
        if self.entries.len() == self.max_entries {
            self.entries.pop_front();
        }
        self.entries.push_back(entry);
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
    pub fn entries(&self) -> impl Iterator<Item = &TranscriptEntry> {
        self.entries.iter()
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

    fn cmd(s: &str) -> TranscriptEntry {
        TranscriptEntry::command(s)
    }

    fn out(s: &str) -> TranscriptEntry {
        TranscriptEntry::output(s)
    }

    fn kinds(t: &Transcript) -> Vec<EntryKind> {
        t.entries().map(|e| e.kind.clone()).collect()
    }

    // --- construction ---

    #[test]
    fn new_transcript_is_empty() {
        let t = Transcript::new(10);
        assert!(t.is_empty());
        assert_eq!(t.len(), 0);
    }

    #[test]
    fn entry_timestamp_is_set() {
        let before = Utc::now();
        let entry = TranscriptEntry::command("ls");
        let after = Utc::now();
        assert!(entry.timestamp >= before);
        assert!(entry.timestamp <= after);
    }

    #[test]
    fn entry_kind_constructors() {
        assert_eq!(
            TranscriptEntry::command("ls").kind,
            EntryKind::Command("ls".into())
        );
        assert_eq!(
            TranscriptEntry::output("hello").kind,
            EntryKind::Output("hello".into())
        );
        assert_eq!(
            TranscriptEntry::ai_response("sure").kind,
            EntryKind::AiResponse("sure".into())
        );
    }

    // --- push / entries ---

    #[test]
    fn push_appends_in_order() {
        let mut t = Transcript::new(10);
        t.push(cmd("a"));
        t.push(out("b"));
        t.push(cmd("c"));
        let got = kinds(&t);
        assert_eq!(
            got,
            vec![
                EntryKind::Command("a".into()),
                EntryKind::Output("b".into()),
                EntryKind::Command("c".into()),
            ]
        );
    }

    // --- sliding-window eviction ---

    #[test]
    fn push_evicts_oldest_at_capacity() {
        let mut t = Transcript::new(3);
        t.push(cmd("a"));
        t.push(cmd("b"));
        t.push(cmd("c"));
        t.push(cmd("d")); // "a" should be evicted
        let texts: Vec<&str> = t.entries().map(|e| e.kind.text()).collect();
        assert_eq!(texts, vec!["b", "c", "d"]);
    }

    #[test]
    fn push_to_capacity_does_not_evict() {
        let mut t = Transcript::new(3);
        t.push(cmd("a"));
        t.push(cmd("b"));
        t.push(cmd("c"));
        assert_eq!(t.len(), 3);
    }

    #[test]
    fn repeated_push_beyond_capacity_keeps_newest() {
        let mut t = Transcript::new(2);
        for i in 0..10usize {
            t.push(cmd(&i.to_string()));
        }
        assert_eq!(t.len(), 2);
        let texts: Vec<&str> = t.entries().map(|e| e.kind.text()).collect();
        assert_eq!(texts, vec!["8", "9"]);
    }

    // --- clear ---

    #[test]
    fn clear_removes_all_entries() {
        let mut t = Transcript::new(10);
        t.push(cmd("a"));
        t.push(cmd("b"));
        t.clear();
        assert!(t.is_empty());
    }

    #[test]
    fn clear_allows_subsequent_push() {
        let mut t = Transcript::new(10);
        t.push(cmd("a"));
        t.clear();
        t.push(cmd("b"));
        let texts: Vec<&str> = t.entries().map(|e| e.kind.text()).collect();
        assert_eq!(texts, vec!["b"]);
    }

    // --- trim ---

    #[test]
    fn trim_zero_is_noop() {
        let mut t = Transcript::new(10);
        t.push(cmd("a"));
        t.push(cmd("b"));
        t.trim(0);
        assert_eq!(t.len(), 2);
    }

    #[test]
    fn trim_drops_oldest_n() {
        let mut t = Transcript::new(10);
        t.push(cmd("a"));
        t.push(cmd("b"));
        t.push(cmd("c"));
        t.trim(2);
        let texts: Vec<&str> = t.entries().map(|e| e.kind.text()).collect();
        assert_eq!(texts, vec!["c"]);
    }

    #[test]
    fn trim_exact_len_clears_all() {
        let mut t = Transcript::new(10);
        t.push(cmd("a"));
        t.push(cmd("b"));
        t.trim(2);
        assert!(t.is_empty());
    }

    #[test]
    fn trim_exceeding_len_clears_all_without_error() {
        let mut t = Transcript::new(10);
        t.push(cmd("a"));
        t.push(cmd("b"));
        t.trim(999);
        assert!(t.is_empty());
    }

    // --- display ---

    #[test]
    fn display_plain_format_is_correct() {
        let entry = TranscriptEntry::command("ls /tmp");
        assert_eq!(entry.display_plain(), "command: ls /tmp");
    }

    #[test]
    fn display_plain_output_tag() {
        let entry = TranscriptEntry::output("hello");
        assert_eq!(entry.display_plain(), "output: hello");
    }

    #[test]
    fn display_with_timestamps_contains_valid_rfc3339() {
        let before = Utc::now();
        let entry = TranscriptEntry::command("ls /tmp");
        let after = Utc::now();
        let d = entry.display_with_timestamps();

        // Format is: [<rfc3339>] <kind>: <text>
        // Extract the timestamp between the first '[' and the first ']'.
        let ts_str = d
            .strip_prefix('[')
            .and_then(|s| s.split_once(']'))
            .map(|(ts, _)| ts)
            .unwrap_or_else(|| panic!("no bracketed timestamp in: {d:?}"));

        // Parse as RFC 3339 — this fails if the value is not a valid timestamp.
        let parsed = ts_str
            .parse::<chrono::DateTime<Utc>>()
            .unwrap_or_else(|e| panic!("timestamp {ts_str:?} is not valid RFC 3339: {e}"));

        // The formatted timestamp uses second precision (SecondsFormat::Secs),
        // so truncate the window boundaries to seconds before comparing.
        use chrono::Timelike as _;
        let before_secs = before.with_nanosecond(0).unwrap();
        let after_secs = after.with_nanosecond(0).unwrap();
        assert!(
            parsed >= before_secs && parsed <= after_secs,
            "timestamp {parsed} is outside expected window [{before_secs}, {after_secs}]"
        );
    }

    #[test]
    fn display_with_timestamps_suffix_is_correct() {
        let entry = TranscriptEntry::command("ls /tmp");
        let d = entry.display_with_timestamps();
        assert!(
            d.ends_with("] command: ls /tmp"),
            "expected suffix '] command: ls /tmp' in: {d:?}"
        );
    }

    #[test]
    fn display_with_timestamps_output_tag() {
        let entry = TranscriptEntry::output("hello");
        let d = entry.display_with_timestamps();
        assert!(
            d.ends_with("] output: hello"),
            "expected suffix '] output: hello' in: {d:?}"
        );
    }
}
