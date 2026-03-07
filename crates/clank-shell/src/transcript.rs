use std::time::SystemTime;

/// The kind of a transcript entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntryKind {
    /// A command typed by the user or issued by a script.
    Command,
    /// Output produced by a command (stdout + stderr combined).
    Output,
    /// A response from the AI model.
    AiResponse,
}

/// A single entry in the transcript.
#[derive(Debug, Clone)]
pub struct Entry {
    pub kind: EntryKind,
    pub timestamp: SystemTime,
    pub text: String,
}

impl Entry {
    /// Approximate token count for this entry (chars / 4).
    pub fn token_estimate(&self) -> usize {
        self.text.len().div_ceil(4)
    }
}

/// The shell's sliding-window session transcript.
///
/// Owns every command typed, every output produced, and every AI response
/// in the current session. The window is bounded by a token budget;
/// `window()` returns only the entries that fit within that budget,
/// counting from the most recent backward.
///
/// Redaction is applied at append time. For Phase 1 no redaction rules
/// are active; the hook is present for Phase 2+.
#[derive(Debug)]
pub struct Transcript {
    entries: Vec<Entry>,
    /// Maximum approximate token count for the sliding window.
    token_budget: usize,
}

impl Transcript {
    /// Default token budget: ~100k tokens.
    pub const DEFAULT_TOKEN_BUDGET: usize = 100_000;

    pub fn new(token_budget: usize) -> Self {
        Self {
            entries: Vec::new(),
            token_budget,
        }
    }

    /// Append an entry to the transcript.
    ///
    /// Empty strings are silently ignored — they add no value to the
    /// model's context and inflate the token count.
    ///
    /// The `redacted` parameter is reserved for Phase 2 redaction rules.
    /// Pass `false` for all entries in Phase 1.
    pub fn append(&mut self, kind: EntryKind, text: impl Into<String>, redacted: bool) {
        if redacted {
            return;
        }
        let text = text.into();
        if text.trim().is_empty() {
            return;
        }
        self.entries.push(Entry {
            kind,
            timestamp: SystemTime::now(),
            text,
        });
    }

    /// Returns the slice of entries that fit within the token budget,
    /// counted from the most recent entry backward.
    pub fn window(&self) -> &[Entry] {
        let mut tokens = 0usize;
        let mut start = self.entries.len();
        for entry in self.entries.iter().rev() {
            let cost = entry.token_estimate();
            if tokens + cost > self.token_budget {
                break;
            }
            tokens += cost;
            start -= 1;
        }
        &self.entries[start..]
    }

    /// Discard all entries.
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Drop the oldest `n` entries.
    pub fn trim(&mut self, n: usize) {
        let drop = n.min(self.entries.len());
        self.entries.drain(..drop);
    }

    /// Returns all entries (for inspection in tests and `context show`).
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// Total number of entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// True if the transcript has no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Render the window as a human/model-readable string.
    ///
    /// Each entry is prefixed with a role label so the model can
    /// distinguish commands, outputs, and prior AI responses.
    pub fn format_for_model(&self) -> String {
        let mut out = String::new();
        for entry in self.window() {
            let label = match entry.kind {
                EntryKind::Command => "$ ",
                EntryKind::Output => "",
                EntryKind::AiResponse => "[assistant] ",
            };
            out.push_str(label);
            out.push_str(&entry.text);
            if !entry.text.ends_with('\n') {
                out.push('\n');
            }
        }
        out
    }

    /// Render the full transcript (all entries, ignoring token budget)
    /// as a human-readable string. Used by `context show`.
    pub fn format_full(&self) -> String {
        let mut out = String::new();
        for entry in &self.entries {
            let label = match entry.kind {
                EntryKind::Command => "$ ",
                EntryKind::Output => "",
                EntryKind::AiResponse => "[assistant] ",
            };
            out.push_str(label);
            out.push_str(&entry.text);
            if !entry.text.ends_with('\n') {
                out.push('\n');
            }
        }
        out
    }
}

impl Default for Transcript {
    fn default() -> Self {
        Self::new(Self::DEFAULT_TOKEN_BUDGET)
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_transcript(budget: usize) -> Transcript {
        Transcript::new(budget)
    }

    #[test]
    fn test_transcript_append_stores_entry() {
        let mut t = make_transcript(Transcript::DEFAULT_TOKEN_BUDGET);
        t.append(EntryKind::Command, "echo hi", false);
        assert_eq!(t.len(), 1);
        assert_eq!(t.entries[0].text, "echo hi");
        assert_eq!(t.entries[0].kind, EntryKind::Command);
    }

    #[test]
    fn test_transcript_append_empty_is_ignored() {
        let mut t = make_transcript(Transcript::DEFAULT_TOKEN_BUDGET);
        t.append(EntryKind::Output, "", false);
        t.append(EntryKind::Output, "   ", false);
        t.append(EntryKind::Output, "\n", false);
        assert_eq!(t.len(), 0);
    }

    #[test]
    fn test_transcript_append_redacted_is_ignored() {
        let mut t = make_transcript(Transcript::DEFAULT_TOKEN_BUDGET);
        t.append(EntryKind::Output, "secret", true);
        assert_eq!(t.len(), 0);
    }

    #[test]
    fn test_transcript_clear_empties_entries() {
        let mut t = make_transcript(Transcript::DEFAULT_TOKEN_BUDGET);
        t.append(EntryKind::Command, "ls", false);
        t.append(EntryKind::Output, "foo\nbar", false);
        t.clear();
        assert_eq!(t.len(), 0);
        assert!(t.is_empty());
    }

    #[test]
    fn test_transcript_trim_drops_oldest_n() {
        let mut t = make_transcript(Transcript::DEFAULT_TOKEN_BUDGET);
        for i in 0..5 {
            t.append(EntryKind::Command, format!("cmd{i}"), false);
        }
        t.trim(2);
        assert_eq!(t.len(), 3);
        assert_eq!(t.entries[0].text, "cmd2");
    }

    #[test]
    fn test_transcript_trim_more_than_len_clears_all() {
        let mut t = make_transcript(Transcript::DEFAULT_TOKEN_BUDGET);
        t.append(EntryKind::Command, "ls", false);
        t.trim(100);
        assert_eq!(t.len(), 0);
    }

    #[test]
    fn test_transcript_window_respects_token_budget() {
        // Budget of 4 tokens ≈ 16 chars. Each entry is ~4 chars → ~1 token.
        // So only the most recent ~4 entries should fit.
        let mut t = make_transcript(4);
        for i in 0..10 {
            // Each entry is exactly 4 chars → 1 token estimate.
            t.append(EntryKind::Command, format!("c{i:0>3}"), false);
        }
        let window = t.window();
        assert!(
            window.len() <= 4,
            "window len {} exceeded budget",
            window.len()
        );
        // Most recent entries are in the window.
        let last = &window[window.len() - 1];
        assert_eq!(last.text, "c009");
    }

    #[test]
    fn test_transcript_window_empty_when_budget_zero() {
        let mut t = make_transcript(0);
        t.append(EntryKind::Command, "ls", false);
        assert_eq!(t.window().len(), 0);
    }

    #[test]
    fn test_transcript_format_for_model_roundtrip() {
        let mut t = make_transcript(Transcript::DEFAULT_TOKEN_BUDGET);
        t.append(EntryKind::Command, "echo hi", false);
        t.append(EntryKind::Output, "hi", false);
        t.append(EntryKind::AiResponse, "Hello!", false);
        let s = t.format_for_model();
        assert!(s.contains("$ echo hi"));
        assert!(s.contains("hi"));
        assert!(s.contains("[assistant] Hello!"));
    }

    #[test]
    fn test_transcript_format_full_includes_all_entries() {
        let mut t = make_transcript(0); // zero budget → window is empty
        t.append(EntryKind::Command, "ls", false);
        // format_full ignores the budget
        let s = t.format_full();
        assert!(s.contains("$ ls"));
        // but format_for_model (window) is empty
        assert!(t.format_for_model().is_empty());
    }

    #[test]
    fn test_transcript_entries_have_timestamps() {
        let mut t = make_transcript(Transcript::DEFAULT_TOKEN_BUDGET);
        let before = SystemTime::now();
        t.append(EntryKind::Command, "ls", false);
        let after = SystemTime::now();
        let ts = t.entries[0].timestamp;
        assert!(ts >= before && ts <= after);
    }
}
