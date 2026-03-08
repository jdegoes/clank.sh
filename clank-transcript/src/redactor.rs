//! Sensitive-value redaction for transcript entries.
//!
//! [`Redactor`] applies a set of pre-compiled regex patterns to arbitrary
//! text, replacing matched sensitive values with `[REDACTED]`. It also
//! supports exact literal scrubbing for values declared in command manifests.

use regex::Regex;

const REDACTED: &str = "[REDACTED]";
// Replacement string that preserves the first capture group (key + separator)
// and replaces only the value portion.
const REDACTED_KEEP_KEY: &str = "${1}[REDACTED]";

/// A compiled set of patterns for scrubbing sensitive values from text.
///
/// Construct with [`Redactor::new`] (or `Redactor::default()`) for the
/// production pattern set or [`Redactor::none`] for a no-op instance.
pub struct Redactor {
    /// Each entry is (compiled regex, replacement string).
    /// Most patterns use `[REDACTED]` directly; patterns with a capture group
    /// for the key name use `${1}[REDACTED]` to preserve the key.
    patterns: Vec<(Regex, &'static str)>,
}

impl Redactor {
    /// Build with the default always-on pattern set.
    ///
    /// Patterns are compiled once here; subsequent [`scrub`] calls are cheap.
    pub fn new() -> Self {
        // Each tuple is (pattern, replacement).
        // Patterns with a capture group for the key/flag use REDACTED_KEEP_KEY
        // so the key name is preserved; others use REDACTED directly.
        let raw: &[(&str, &str)] = &[
            // Config-secret: capture the key+separator in group 1, replace
            // only the value. Covers .env, TOML, YAML, and shell exports.
            (
                r#"(?i)([a-z0-9_]*(?:token|secret|password|passwd|pwd|api[_-]?key|credential|auth[_-]?key)[a-z0-9_]*\s*[=:]\s*)["']?[^\s"',;\]}\n][^"',;\]}\n]*["']?"#,
                REDACTED_KEEP_KEY,
            ),
            // Generic CLI flags: capture flag name in group 1, replace value.
            (
                r"(?i)(--(?:api[_-]?key|token|secret|password|passwd|pwd))[=\s]+\S+",
                REDACTED_KEEP_KEY,
            ),
            // AWS access key ID (full replacement — the whole token is the secret)
            (r"AKIA[0-9A-Z]{16}", REDACTED),
            // AWS secret access key
            (r"(?i)aws.{0,20}secret.{0,20}[0-9a-zA-Z/+]{40}", REDACTED),
            // GitHub personal access token
            (r"ghp_[a-zA-Z0-9]{36}", REDACTED),
            // GitHub fine-grained PAT
            (r"github_pat_[a-zA-Z0-9_]{82}", REDACTED),
            // Generic JWT
            (
                r"ey[a-zA-Z0-9_-]{10,}\.ey[a-zA-Z0-9_-]{10,}\.[a-zA-Z0-9_-]+",
                REDACTED,
            ),
            // PEM private key header
            (r"-----BEGIN [A-Z ]* PRIVATE KEY-----", REDACTED),
            // HTTP Authorization Bearer token
            (r"(?i)bearer\s+[a-zA-Z0-9_\-\.]{16,}", REDACTED),
        ];

        let patterns = raw
            .iter()
            .map(|(p, r)| {
                (
                    Regex::new(p).expect("built-in redaction pattern must be valid"),
                    *r,
                )
            })
            .collect();

        Self { patterns }
    }

    /// Build with no patterns — every input passes through unchanged.
    ///
    /// Used in tests to avoid false positives on synthetic data.
    /// Build with no patterns — every input passes through unchanged.
    ///
    /// Used in tests to avoid false positives on synthetic data.
    pub fn none() -> Self {
        Self { patterns: vec![] }
    }
}

impl Default for Redactor {
    fn default() -> Self {
        Self::new()
    }
}

impl Redactor {
    /// Scrub `text` by replacing all pattern matches with `[REDACTED]`.
    ///
    /// Patterns are applied in order; later patterns see the already-scrubbed
    /// text. If no patterns match, the original `text` is returned as a new
    /// `String` (no allocation-free shortcut — callers should not depend on
    /// identity comparison).
    pub fn scrub(&self, text: &str) -> String {
        let mut result = text.to_owned();
        for (pattern, replacement) in &self.patterns {
            let scrubbed = pattern.replace_all(&result, *replacement);
            if matches!(scrubbed, std::borrow::Cow::Owned(_)) {
                result = scrubbed.into_owned();
            }
        }
        result
    }

    /// Scrub the text of a [`super::TranscriptEntry`], returning a new entry
    /// with the same timestamp and kind but with the text replaced by the
    /// scrubbed version.
    pub(crate) fn scrub_entry(&self, entry: super::TranscriptEntry) -> super::TranscriptEntry {
        let scrubbed = self.scrub(entry.kind.text());
        // Only allocate a new entry if the text actually changed.
        if scrubbed == entry.kind.text() {
            return entry;
        }
        let kind = match entry.kind {
            super::EntryKind::Command(_) => super::EntryKind::Command(scrubbed),
            super::EntryKind::Output(_) => super::EntryKind::Output(scrubbed),
            super::EntryKind::AiResponse(_) => super::EntryKind::AiResponse(scrubbed),
        };
        super::TranscriptEntry {
            timestamp: entry.timestamp,
            kind,
        }
    }

    /// Scrub exact literal values from `text`.
    ///
    /// Used for `redaction_rules` in command manifests: the caller has already
    /// extracted the values of declared-secret arguments and passes them here
    /// for exact-match replacement. Each literal is replaced with `[REDACTED]`
    /// wherever it appears in the text.
    ///
    /// Empty literals are ignored (no replacement performed).
    pub fn scrub_literals(&self, text: &str, literals: &[&str]) -> String {
        let mut result = text.to_owned();
        for &literal in literals {
            if literal.is_empty() {
                continue;
            }
            result = result.replace(literal, REDACTED);
        }
        result
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn redactor() -> Redactor {
        Redactor::default()
    }

    // --- config-secret pattern ---

    #[test]
    fn env_var_uppercase_password() {
        assert_eq!(
            redactor().scrub("DB_PASSWORD=hunter2"),
            "DB_PASSWORD=[REDACTED]"
        );
    }

    #[test]
    fn env_var_uppercase_token() {
        assert_eq!(
            redactor().scrub("STRIPE_API_TOKEN=sk_live_abc123"),
            "STRIPE_API_TOKEN=[REDACTED]"
        );
    }

    #[test]
    fn env_var_uppercase_secret() {
        assert_eq!(
            redactor().scrub("MY_APP_SECRET=xyz789"),
            "MY_APP_SECRET=[REDACTED]"
        );
    }

    #[test]
    fn env_var_with_export_prefix() {
        assert_eq!(
            redactor().scrub("export GITHUB_TOKEN=ghp_abc"),
            "export GITHUB_TOKEN=[REDACTED]"
        );
    }

    #[test]
    fn toml_lowercase_password_quoted() {
        assert_eq!(
            redactor().scrub(r#"password = "hunter2""#),
            r#"password = [REDACTED]"#
        );
    }

    #[test]
    fn toml_lowercase_api_key_quoted() {
        assert_eq!(
            redactor().scrub(r#"api_key = "sk_live_abc""#),
            r#"api_key = [REDACTED]"#
        );
    }

    #[test]
    fn toml_lowercase_secret_token() {
        assert_eq!(
            redactor().scrub(r#"secret_token = "xyz789""#),
            r#"secret_token = [REDACTED]"#
        );
    }

    #[test]
    fn yaml_lowercase_password_bare() {
        assert_eq!(
            redactor().scrub("password: hunter2"),
            "password: [REDACTED]"
        );
    }

    #[test]
    fn yaml_lowercase_auth_key_quoted() {
        assert_eq!(
            redactor().scrub(r#"auth_key: "abc123""#),
            r#"auth_key: [REDACTED]"#
        );
    }

    #[test]
    fn empty_assignment_not_redacted() {
        // No value to protect — must not redact.
        let r = redactor();
        assert_eq!(r.scrub("password="), "password=");
        assert_eq!(r.scrub("api_key:"), "api_key:");
    }

    #[test]
    fn non_sensitive_name_not_redacted() {
        // MYAPP= does not contain any sensitive keyword.
        assert_eq!(redactor().scrub("MYAPP=hunter2"), "MYAPP=hunter2");
    }

    // --- generic CLI flag pattern ---

    #[test]
    fn cli_flag_password_space() {
        let result = redactor().scrub("login --password secret123");
        assert!(result.contains("[REDACTED]"), "got: {result}");
        assert!(!result.contains("secret123"), "got: {result}");
    }

    #[test]
    fn cli_flag_token_equals() {
        let result = redactor().scrub("curl --token=abc123def456");
        assert!(result.contains("[REDACTED]"), "got: {result}");
    }

    // --- AWS patterns ---

    #[test]
    fn aws_access_key_id() {
        let result = redactor().scrub("echo AKIA1234567890ABCDEF");
        assert!(result.contains("[REDACTED]"), "got: {result}");
        assert!(!result.contains("AKIA1234567890ABCDEF"), "got: {result}");
    }

    // --- GitHub token patterns ---

    #[test]
    fn github_personal_access_token() {
        let token = "ghp_".to_owned() + &"a".repeat(36);
        let result = redactor().scrub(&format!("echo {token}"));
        assert!(result.contains("[REDACTED]"), "got: {result}");
        assert!(!result.contains(&token), "got: {result}");
    }

    // --- JWT pattern ---

    #[test]
    fn jwt_three_segments() {
        let jwt = "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJ1c2VyMTIzIn0.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c";
        let result = redactor().scrub(&format!("Authorization: Bearer {jwt}"));
        assert!(result.contains("[REDACTED]"), "got: {result}");
    }

    // --- PEM private key ---

    #[test]
    fn pem_private_key_header() {
        let result = redactor().scrub("-----BEGIN RSA PRIVATE KEY-----");
        assert!(result.contains("[REDACTED]"), "got: {result}");
    }

    // --- Bearer token ---

    #[test]
    fn bearer_token_in_header() {
        let result = redactor().scrub("Authorization: Bearer abc123def456ghi789jkl");
        assert!(result.contains("[REDACTED]"), "got: {result}");
    }

    // --- no false positives ---

    #[test]
    fn normal_text_unchanged() {
        let r = redactor();
        assert_eq!(r.scrub("echo hello"), "echo hello");
        assert_eq!(r.scrub("ls /tmp"), "ls /tmp");
        assert_eq!(r.scrub("echo a"), "echo a");
    }

    #[test]
    fn redactor_none_passes_everything_through() {
        let r = Redactor::none();
        let sensitive = "DB_PASSWORD=hunter2 AKIA1234567890ABCDEF";
        assert_eq!(r.scrub(sensitive), sensitive);
    }

    // --- scrub_literals ---

    #[test]
    fn scrub_literals_replaces_exact_value() {
        let r = Redactor::none();
        assert_eq!(
            r.scrub_literals("model add --key sk_live_abc123", &["sk_live_abc123"]),
            "model add --key [REDACTED]"
        );
    }

    #[test]
    fn scrub_literals_multiple_occurrences() {
        let r = Redactor::none();
        assert_eq!(
            r.scrub_literals("a=secret b=secret", &["secret"]),
            "a=[REDACTED] b=[REDACTED]"
        );
    }

    #[test]
    fn scrub_literals_empty_literal_ignored() {
        let r = Redactor::none();
        assert_eq!(r.scrub_literals("hello world", &[""]), "hello world");
    }

    #[test]
    fn scrub_literals_empty_list_unchanged() {
        let r = Redactor::none();
        assert_eq!(r.scrub_literals("hello world", &[]), "hello world");
    }
}
