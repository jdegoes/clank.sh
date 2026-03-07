/// A single entry in the shell transcript.
///
/// The transcript records every command typed and every output produced during
/// a shell session. It is the AI's context window — what `ask` reads.
#[derive(Debug, Clone, PartialEq)]
pub enum TranscriptEntry {
    /// A command typed by the operator (human or AI).
    Command { input: String },
    /// Text written to stdout by a command.
    Output { text: String },
    /// Text written to stderr by a command.
    Error { text: String },
    /// A response from an AI model. Populated by `ask` (future task).
    AiResponse { text: String },
    /// A compacted summary replacing older entries.
    /// Never appended directly — produced internally by [`Transcript::compact`].
    Summary { text: String },
}

impl TranscriptEntry {
    /// The semantic label used in [`Transcript::format_for_model`] output.
    fn label(&self) -> &'static str {
        match self {
            TranscriptEntry::Command { .. } => "input",
            TranscriptEntry::Output { .. } => "output",
            TranscriptEntry::Error { .. } => "error",
            TranscriptEntry::AiResponse { .. } => "ai",
            TranscriptEntry::Summary { .. } => "summary",
        }
    }

    /// The text content of this entry.
    fn text(&self) -> &str {
        match self {
            TranscriptEntry::Command { input } => input,
            TranscriptEntry::Output { text }
            | TranscriptEntry::Error { text }
            | TranscriptEntry::AiResponse { text }
            | TranscriptEntry::Summary { text } => text,
        }
    }

    /// Approximate token count for this entry (1 token ≈ 4 characters).
    fn approx_tokens(&self) -> usize {
        (self.text().len() / 4).max(1)
    }
}

/// The outcome of running a single command through `ClankShell`.
#[derive(Debug)]
pub struct CommandOutcome {
    /// Text written to stdout by the command.
    pub stdout: String,
    /// Text written to stderr by the command.
    pub stderr: String,
    /// Exit code of the command.
    pub exit_code: u8,
}

/// The shell's session transcript — a first-class value owned by `ClankShell`.
///
/// Records every command input, its stdout, and its stderr in order. Provides
/// the context window that `ask` sends to the AI model. When the total
/// approximate token count exceeds the budget, the oldest entries are
/// automatically compacted into a [`TranscriptEntry::Summary`].
pub struct Transcript {
    entries: Vec<TranscriptEntry>,
    /// Maximum approximate token budget. When exceeded, compaction fires.
    max_tokens: usize,
}

/// After compaction, the window is trimmed to this fraction of the budget (75%).
const WINDOW_COMPACTION_RATIO: f64 = 0.75;

/// Default token budget (8,000 tokens ≈ ~32,000 characters).
const DEFAULT_TOKEN_BUDGET: usize = 8_000;

impl Default for Transcript {
    fn default() -> Self {
        Self::with_default_budget()
    }
}

impl std::fmt::Debug for Transcript {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Transcript")
            .field("entries", &self.entries.len())
            .field("max_tokens", &self.max_tokens)
            .finish()
    }
}

impl Transcript {
    /// Create a new transcript with the given token budget.
    pub fn new(max_tokens: usize) -> Self {
        Self {
            entries: Vec::new(),
            max_tokens,
        }
    }

    /// Create a new transcript with the default token budget (8,000 tokens).
    pub fn with_default_budget() -> Self {
        Self::new(DEFAULT_TOKEN_BUDGET)
    }

    /// Record a command line typed by the operator.
    pub fn push_command(&mut self, input: &str) {
        self.append(TranscriptEntry::Command {
            input: input.to_string(),
        });
    }

    /// Record text written to stdout by a command.
    pub fn push_output(&mut self, text: &str) {
        if !text.is_empty() {
            self.append(TranscriptEntry::Output {
                text: text.to_string(),
            });
        }
    }

    /// Record text written to stderr by a command.
    pub fn push_error(&mut self, text: &str) {
        if !text.is_empty() {
            self.append(TranscriptEntry::Error {
                text: text.to_string(),
            });
        }
    }

    /// Record an AI model response. Called by `ask` (future task).
    pub fn push_ai_response(&mut self, text: &str) {
        if !text.is_empty() {
            self.append(TranscriptEntry::AiResponse {
                text: text.to_string(),
            });
        }
    }

    /// Discard all transcript entries. The AI starts fresh on the next `ask`.
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Drop the oldest `n` entries from the transcript.
    /// If `n` is greater than the number of entries, all entries are dropped.
    pub fn trim(&mut self, n: usize) {
        let to_remove = n.min(self.entries.len());
        self.entries.drain(..to_remove);
    }

    /// Format the transcript with semantic entry-kind labels for the AI model.
    ///
    /// Each entry is prefixed with its kind in brackets so the model can
    /// unambiguously distinguish commands from output, stderr, and AI responses.
    ///
    /// Format:
    /// ```text
    /// [input] echo hello
    /// [output] hello
    /// [error] bash: foo: not found
    /// [ai] This is a shell.
    /// [summary] [earlier transcript compacted]
    /// ```
    pub fn format_for_model(&self) -> String {
        let mut out = String::new();
        for entry in &self.entries {
            out.push('[');
            out.push_str(entry.label());
            out.push_str("] ");
            out.push_str(entry.text().trim_end_matches('\n'));
            out.push('\n');
        }
        out
    }

    /// Format the transcript in plain shell format for human display.
    ///
    /// Used by `context show`. Prefer [`format_for_model`](Self::format_for_model)
    /// when passing to an AI model.
    pub fn as_string(&self) -> String {
        let mut out = String::new();
        for entry in &self.entries {
            match entry {
                TranscriptEntry::Command { input } => {
                    out.push_str("$ ");
                    out.push_str(input);
                    out.push('\n');
                }
                TranscriptEntry::Output { text }
                | TranscriptEntry::Error { text }
                | TranscriptEntry::AiResponse { text }
                | TranscriptEntry::Summary { text } => {
                    out.push_str(text);
                    if !text.ends_with('\n') {
                        out.push('\n');
                    }
                }
            }
        }
        out
    }

    /// Approximate token count for all entries in the current window.
    pub fn approximate_tokens(&self) -> usize {
        self.entries
            .iter()
            .map(TranscriptEntry::approx_tokens)
            .sum()
    }

    /// Returns true if the transcript has no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Returns the number of entries in the transcript.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    // ── Internal ─────────────────────────────────────────────────────────────

    fn append(&mut self, entry: TranscriptEntry) {
        self.entries.push(entry);
        if self.approximate_tokens() > self.max_tokens {
            self.compact();
        }
    }

    /// Replace leading entries with a single Summary so total token count
    /// falls to ≤ `WINDOW_COMPACTION_RATIO * max_tokens`.
    ///
    /// Always preserves at least the most recent entry.
    fn compact(&mut self) {
        let target = (self.max_tokens as f64 * WINDOW_COMPACTION_RATIO) as usize;
        let max_cut = self.entries.len().saturating_sub(1);
        if max_cut == 0 {
            return;
        }

        // Fixed-size summary placeholder (1 token). The content of the compacted
        // entries is intentionally not preserved verbatim — the sliding window
        // design means older history is summarised and discarded. A future `ask`-
        // backed `context summarize` will generate a meaningful AI summary.
        let summary_text = "[earlier transcript compacted]\n".to_string();
        let summary_tokens = (summary_text.len() / 4).max(1);

        // Walk forward dropping entries until remaining + summary fits in target.
        let mut cut = 0usize;
        while cut < max_cut {
            let remaining_tokens: usize = self.entries[cut..]
                .iter()
                .map(TranscriptEntry::approx_tokens)
                .sum();
            if summary_tokens + remaining_tokens <= target {
                break;
            }
            cut += 1;
        }

        if cut == 0 {
            return;
        }

        self.entries.drain(..cut);
        self.entries
            .insert(0, TranscriptEntry::Summary { text: summary_text });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Basic operations ──────────────────────────────────────────────────────

    #[test]
    fn new_transcript_is_empty() {
        let t = Transcript::with_default_budget();
        assert!(t.is_empty());
        assert_eq!(t.len(), 0);
        assert_eq!(t.approximate_tokens(), 0);
    }

    #[test]
    fn push_command_records_entry() {
        let mut t = Transcript::with_default_budget();
        t.push_command("echo hello");
        assert_eq!(t.len(), 1);
    }

    #[test]
    fn push_output_records_entry() {
        let mut t = Transcript::with_default_budget();
        t.push_output("hello\n");
        assert_eq!(t.len(), 1);
    }

    #[test]
    fn push_error_records_entry() {
        let mut t = Transcript::with_default_budget();
        t.push_error("bash: foo: not found\n");
        assert_eq!(t.len(), 1);
        assert!(matches!(t.entries[0], TranscriptEntry::Error { .. }));
    }

    #[test]
    fn push_empty_output_is_ignored() {
        let mut t = Transcript::with_default_budget();
        t.push_output("");
        assert!(t.is_empty());
    }

    #[test]
    fn push_empty_error_is_ignored() {
        let mut t = Transcript::with_default_budget();
        t.push_error("");
        assert!(t.is_empty());
    }

    #[test]
    fn clear_empties_transcript() {
        let mut t = Transcript::with_default_budget();
        t.push_command("echo hello");
        t.push_output("hello\n");
        t.clear();
        assert!(t.is_empty());
    }

    #[test]
    fn trim_drops_oldest_n_entries() {
        let mut t = Transcript::with_default_budget();
        t.push_command("first");
        t.push_output("out1\n");
        t.push_command("second");
        t.push_output("out2\n");
        t.trim(2);
        assert_eq!(t.len(), 2);
        assert_eq!(
            t.entries[0],
            TranscriptEntry::Command {
                input: "second".to_string()
            }
        );
    }

    #[test]
    fn trim_more_than_length_clears_all() {
        let mut t = Transcript::with_default_budget();
        t.push_command("echo hi");
        t.trim(100);
        assert!(t.is_empty());
    }

    // ── Rendering ─────────────────────────────────────────────────────────────

    #[test]
    fn render_uses_semantic_labels() {
        let mut t = Transcript::with_default_budget();
        t.push_command("echo hello");
        t.push_output("hello\n");
        t.push_error("warning\n");
        let r = t.format_for_model();
        assert_eq!(r, "[input] echo hello\n[output] hello\n[error] warning\n");
    }

    #[test]
    fn as_string_formats_correctly() {
        let mut t = Transcript::with_default_budget();
        t.push_command("echo hello");
        t.push_output("hello\n");
        t.push_command("true");
        let s = t.as_string();
        assert_eq!(s, "$ echo hello\nhello\n$ true\n");
    }

    #[test]
    fn as_string_adds_newline_to_output_without_one() {
        let mut t = Transcript::with_default_budget();
        t.push_command("echo hi");
        t.push_output("hi"); // no trailing newline
        let s = t.as_string();
        assert!(s.ends_with("hi\n"));
    }

    #[test]
    fn ai_response_is_included_in_render() {
        let mut t = Transcript::with_default_budget();
        t.push_command("ask what is this");
        t.push_ai_response("This is a shell.");
        let r = t.format_for_model();
        assert!(r.contains("[ai] This is a shell."));
    }

    // ── Token counting ────────────────────────────────────────────────────────

    #[test]
    fn token_count_approximation() {
        let mut t = Transcript::with_default_budget();
        t.push_output("abcd"); // 4 chars → 1 token
        t.push_output("abcdefgh"); // 8 chars → 2 tokens
        assert_eq!(t.approximate_tokens(), 3);
    }

    // ── Compaction ────────────────────────────────────────────────────────────

    #[test]
    fn compaction_fires_when_budget_exceeded() {
        // Budget 100 tokens. Each entry is 4 chars → 1 token.
        // After 101 entries compaction must fire.
        let mut t = Transcript::new(100);
        for _ in 0..101 {
            t.push_output("abcd");
        }
        assert!(
            t.entries
                .first()
                .is_some_and(|e| matches!(e, TranscriptEntry::Summary { .. })),
            "expected a Summary entry after compaction"
        );
    }

    #[test]
    fn compaction_preserves_most_recent_entry() {
        let mut t = Transcript::new(40);
        for i in 0..50 {
            let s = format!("{i:03}x"); // 4 chars → 1 token
            t.push_output(&s);
        }
        let last = t.entries.last().unwrap();
        assert!(
            matches!(last, TranscriptEntry::Output { text } if text == "049x"),
            "last entry should be the most recently appended output"
        );
    }

    #[test]
    fn compaction_token_count_within_budget_after_compact() {
        let mut t = Transcript::new(100);
        for _ in 0..150 {
            t.push_output("abcd");
        }
        // After compaction, token count should be ≤ max_tokens
        assert!(
            t.approximate_tokens() <= 100,
            "token count {} should be ≤ 100 after compaction",
            t.approximate_tokens()
        );
    }
}
