//! Transcript — the shell's sliding-window session history and AI context window.
//!
//! The transcript is an ordered sequence of [`Entry`] values. The REPL appends
//! one [`EntryKind::Input`] entry per command typed, one [`EntryKind::Output`]
//! entry per command's captured stdout, and one [`EntryKind::Error`] entry per
//! command's captured stderr. AI responses will be appended as
//! [`EntryKind::AiResponse`] when `ask` is implemented.
//!
//! When the total approximate token count approaches `max_tokens`, the leading
//! edge is compacted: the oldest non-[`EntryKind::Summary`] entries are replaced
//! with a single `Summary` entry. The boundary between summarized and live
//! history is always explicit.
//!
//! # Token counting
//!
//! Tokens are approximated as `text.len() / 4`. This is a deliberate
//! simplification — real tokenizers are provider-specific and would be a
//! compile-time dependency. The approximation is good enough for planning
//! the sliding window; the actual limit will be enforced by the provider
//! layer when `ask` is implemented.

/// The kind of a transcript entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntryKind {
    /// A line typed by the user.
    Input,
    /// Text written to stdout by a command.
    Output,
    /// Text written to stderr by a command.
    Error,
    /// A response from the AI model.
    AiResponse,
    /// A compacted summary replacing older entries. Never appended directly —
    /// produced internally by the compaction logic.
    Summary,
}

impl EntryKind {
    fn label(&self) -> &'static str {
        match self {
            EntryKind::Input => "input",
            EntryKind::Output => "output",
            EntryKind::Error => "error",
            EntryKind::AiResponse => "ai",
            EntryKind::Summary => "summary",
        }
    }
}

/// A single entry in the transcript.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub kind: EntryKind,
    pub text: String,
}

impl Entry {
    fn approx_tokens(&self) -> usize {
        // 1 token ≈ 4 characters (rough heuristic; see module doc).
        (self.text.len() / 4).max(1)
    }
}

/// The shell's sliding-window transcript.
///
/// See module-level documentation for the design.
pub struct Transcript {
    entries: Vec<Entry>,
    max_tokens: usize,
}

/// Fraction of `max_tokens` to target after compaction (75 %).
const COMPACTION_TARGET: f64 = 0.75;

impl Transcript {
    /// Create a new transcript with the given token budget.
    pub fn new(max_tokens: usize) -> Self {
        Self {
            entries: Vec::new(),
            max_tokens,
        }
    }

    /// Create a new transcript with the default token budget (8 000 tokens).
    pub fn default_budget() -> Self {
        Self::new(8_000)
    }

    /// Append an entry. If the budget is exceeded after appending, the leading
    /// edge is compacted automatically.
    pub fn append(&mut self, kind: EntryKind, text: impl Into<String>) {
        let text = text.into();
        if text.is_empty() {
            return;
        }
        self.entries.push(Entry { kind, text });
        if self.token_count() > self.max_tokens {
            self.compact();
        }
    }

    /// Return all entries in the current window, oldest first.
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// Approximate token count for the current window.
    pub fn token_count(&self) -> usize {
        self.entries.iter().map(Entry::approx_tokens).sum()
    }

    /// Discard all entries.
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Drop the oldest `n` entries. If `n >= len`, the transcript is cleared.
    pub fn trim(&mut self, n: usize) {
        if n >= self.entries.len() {
            self.entries.clear();
        } else {
            self.entries.drain(..n);
        }
    }

    /// Render the full window as a plain string suitable for use as a model
    /// context. Each entry is prefixed with its kind label in brackets.
    ///
    /// Example output:
    /// ```text
    /// [input] echo hello
    /// [output] hello
    /// [summary] [summary of prior transcript]
    /// ```
    pub fn render(&self) -> String {
        let mut out = String::new();
        for entry in &self.entries {
            out.push('[');
            out.push_str(entry.kind.label());
            out.push_str("] ");
            out.push_str(entry.text.trim_end_matches('\n'));
            out.push('\n');
        }
        out
    }

    // -----------------------------------------------------------------------
    // Internal
    // -----------------------------------------------------------------------

    /// Replace the leading entries with a single Summary entry so that the
    /// total token count falls to ≤ `COMPACTION_TARGET * max_tokens`.
    ///
    /// At least one non-summary entry is always preserved (the most recent
    /// one). If the transcript is so large that even the summary alone exceeds
    /// the budget, we still compact as much as possible — we never make the
    /// transcript larger by refusing to compact.
    fn compact(&mut self) {
        let target = (self.max_tokens as f64 * COMPACTION_TARGET) as usize;

        // Must always leave at least 1 entry (the most recent).
        let max_cut = self.entries.len().saturating_sub(1);
        if max_cut == 0 {
            return;
        }

        let mut summary_text = String::from("[summary of prior transcript]\n");
        let mut cut = 0usize;

        // Keep absorbing leading entries until the remaining entries plus the
        // new summary entry fit within the target, but never cut the last entry.
        while cut < max_cut {
            // Cost of the summary entry we would insert.
            let summary_tokens = (summary_text.len() / 4).max(1);
            // Cost of the remaining (not-yet-cut) entries.
            let remaining_tokens: usize =
                self.entries[cut..].iter().map(Entry::approx_tokens).sum();

            if summary_tokens + remaining_tokens <= target {
                break;
            }

            let entry = &self.entries[cut];
            if matches!(entry.kind, EntryKind::Summary) {
                summary_text.push_str(&entry.text);
            } else {
                summary_text.push('[');
                summary_text.push_str(entry.kind.label());
                summary_text.push_str("] ");
                summary_text.push_str(entry.text.trim_end_matches('\n'));
                summary_text.push('\n');
            }
            cut += 1;
        }

        if cut == 0 {
            return;
        }

        self.entries.drain(..cut);
        self.entries.insert(
            0,
            Entry {
                kind: EntryKind::Summary,
                text: summary_text,
            },
        );
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn append_records_in_order() {
        let mut t = Transcript::default_budget();
        t.append(EntryKind::Input, "echo hello");
        t.append(EntryKind::Output, "hello");
        let entries = t.entries();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].kind, EntryKind::Input);
        assert_eq!(entries[0].text, "echo hello");
        assert_eq!(entries[1].kind, EntryKind::Output);
        assert_eq!(entries[1].text, "hello");
    }

    #[test]
    fn empty_text_not_appended() {
        let mut t = Transcript::default_budget();
        t.append(EntryKind::Output, "");
        assert_eq!(t.entries().len(), 0);
    }

    #[test]
    fn render_format() {
        let mut t = Transcript::default_budget();
        t.append(EntryKind::Input, "echo hello");
        t.append(EntryKind::Output, "hello\n");
        let rendered = t.render();
        assert_eq!(rendered, "[input] echo hello\n[output] hello\n");
    }

    #[test]
    fn clear_empties_transcript() {
        let mut t = Transcript::default_budget();
        t.append(EntryKind::Input, "echo hello");
        t.clear();
        assert!(t.entries().is_empty());
        assert_eq!(t.token_count(), 0);
    }

    #[test]
    fn trim_drops_oldest_n() {
        let mut t = Transcript::default_budget();
        t.append(EntryKind::Input, "a");
        t.append(EntryKind::Input, "b");
        t.append(EntryKind::Input, "c");
        t.trim(2);
        assert_eq!(t.entries().len(), 1);
        assert_eq!(t.entries()[0].text, "c");
    }

    #[test]
    fn trim_more_than_len_clears() {
        let mut t = Transcript::default_budget();
        t.append(EntryKind::Input, "a");
        t.trim(100);
        assert!(t.entries().is_empty());
    }

    #[test]
    fn compaction_fires_when_budget_exceeded() {
        // Budget of 100 tokens. Each entry is 4 chars → 1 token.
        // After 101 entries (101 tokens > 100) compaction must have fired,
        // replacing leading entries with a Summary.
        let mut t = Transcript::new(100);
        for _ in 0..101 {
            t.append(EntryKind::Input, "abcd"); // 4 chars → 1 token
        }
        // After compaction there must be a Summary entry at position 0.
        assert!(
            t.entries()
                .first()
                .is_some_and(|e| e.kind == EntryKind::Summary),
            "expected a Summary entry after compaction, entries: {:?}",
            t.entries().iter().map(|e| &e.kind).collect::<Vec<_>>()
        );
        // There must be at least one non-summary entry remaining (last entry
        // was appended just before compaction ran).
        assert!(
            t.entries().iter().any(|e| e.kind != EntryKind::Summary),
            "at least one non-summary entry should remain after compaction"
        );
    }

    #[test]
    fn compaction_preserves_recent_entries() {
        // Budget large enough that recent entries survive compaction.
        // 50 entries × 1 token each = 50 tokens. Budget is 40.
        // Compaction must keep recent entries and only summarise the oldest.
        let mut t = Transcript::new(40);
        for i in 0..50 {
            t.append(EntryKind::Input, format!("{i:03}x")); // 4 chars → 1 token
        }
        // The last entry must still be present as a non-summary entry.
        let last = t.entries().last().unwrap();
        assert_eq!(
            last.kind,
            EntryKind::Input,
            "last entry should be Input, not Summary"
        );
        assert_eq!(last.text, "049x");
    }

    #[test]
    fn token_count_approximation() {
        let mut t = Transcript::default_budget();
        t.append(EntryKind::Input, "abcd"); // 4 chars → 1 token
        t.append(EntryKind::Input, "abcdefgh"); // 8 chars → 2 tokens
        assert_eq!(t.token_count(), 3);
    }
}
