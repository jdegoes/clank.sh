/// A single entry in the shell transcript.
///
/// The transcript records every command typed and every output produced during
/// a shell session. It is the AI's context window — what `ask` reads.
#[derive(Debug, Clone, PartialEq)]
pub enum TranscriptEntry {
    /// A command typed by the operator (human or AI).
    Command { input: String },
    /// Output produced by a command (stdout + stderr combined, as rendered).
    Output { text: String },
    /// A response from an AI model. Populated by `ask` (future task).
    AiResponse { text: String },
}

/// The outcome of running a single command through `ClankShell`.
#[derive(Debug)]
pub struct CommandOutcome {
    /// Combined stdout + stderr captured from the command.
    pub output: String,
    /// Exit code of the command.
    pub exit_code: u8,
}

/// The shell's session transcript — a first-class value owned by `ClankShell`.
///
/// Records every command input and its output in order. Provides the context
/// window that `ask` sends to the AI model.
#[derive(Debug, Default)]
pub struct Transcript {
    entries: Vec<TranscriptEntry>,
}

impl Transcript {
    /// Create a new, empty transcript.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a command line typed by the operator.
    pub fn push_command(&mut self, input: &str) {
        self.entries.push(TranscriptEntry::Command {
            input: input.to_string(),
        });
    }

    /// Record output produced by a command.
    pub fn push_output(&mut self, text: &str) {
        if !text.is_empty() {
            self.entries.push(TranscriptEntry::Output {
                text: text.to_string(),
            });
        }
    }

    /// Record an AI model response. Called by `ask` (future task).
    pub fn push_ai_response(&mut self, text: &str) {
        self.entries.push(TranscriptEntry::AiResponse {
            text: text.to_string(),
        });
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

    /// Render the full transcript as a string, suitable for passing to `ask`.
    ///
    /// Format:
    /// ```text
    /// $ echo hello
    /// hello
    /// $ ls
    /// Cargo.toml
    /// ```
    pub fn as_string(&self) -> String {
        let mut out = String::new();
        for entry in &self.entries {
            match entry {
                TranscriptEntry::Command { input } => {
                    out.push_str("$ ");
                    out.push_str(input);
                    out.push('\n');
                }
                TranscriptEntry::Output { text } => {
                    out.push_str(text);
                    if !text.ends_with('\n') {
                        out.push('\n');
                    }
                }
                TranscriptEntry::AiResponse { text } => {
                    out.push_str(text);
                    if !text.ends_with('\n') {
                        out.push('\n');
                    }
                }
            }
        }
        out
    }

    /// Returns true if the transcript has no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Returns the number of entries in the transcript.
    pub fn len(&self) -> usize {
        self.entries.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_transcript_is_empty() {
        let t = Transcript::new();
        assert!(t.is_empty());
        assert_eq!(t.len(), 0);
    }

    #[test]
    fn push_command_records_entry() {
        let mut t = Transcript::new();
        t.push_command("echo hello");
        assert_eq!(t.len(), 1);
    }

    #[test]
    fn push_output_records_entry() {
        let mut t = Transcript::new();
        t.push_output("hello\n");
        assert_eq!(t.len(), 1);
    }

    #[test]
    fn push_empty_output_is_ignored() {
        let mut t = Transcript::new();
        t.push_output("");
        assert!(t.is_empty());
    }

    #[test]
    fn clear_empties_transcript() {
        let mut t = Transcript::new();
        t.push_command("echo hello");
        t.push_output("hello\n");
        t.clear();
        assert!(t.is_empty());
    }

    #[test]
    fn trim_drops_oldest_n_entries() {
        let mut t = Transcript::new();
        t.push_command("first");
        t.push_output("out1\n");
        t.push_command("second");
        t.push_output("out2\n");
        t.trim(2);
        assert_eq!(t.len(), 2);
        // The remaining entries should be the last two
        assert_eq!(
            t.entries[0],
            TranscriptEntry::Command {
                input: "second".to_string()
            }
        );
    }

    #[test]
    fn trim_more_than_length_clears_all() {
        let mut t = Transcript::new();
        t.push_command("echo hi");
        t.trim(100);
        assert!(t.is_empty());
    }

    #[test]
    fn as_string_formats_correctly() {
        let mut t = Transcript::new();
        t.push_command("echo hello");
        t.push_output("hello\n");
        t.push_command("true");
        let s = t.as_string();
        assert_eq!(s, "$ echo hello\nhello\n$ true\n");
    }

    #[test]
    fn as_string_adds_newline_to_output_without_one() {
        let mut t = Transcript::new();
        t.push_command("echo hi");
        t.push_output("hi"); // no trailing newline
        let s = t.as_string();
        assert!(s.ends_with("hi\n"));
    }

    #[test]
    fn ai_response_is_included_in_as_string() {
        let mut t = Transcript::new();
        t.push_command("ask what is this");
        t.push_ai_response("This is a shell.");
        let s = t.as_string();
        assert!(s.contains("This is a shell."));
    }
}
