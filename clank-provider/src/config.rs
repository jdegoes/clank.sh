//! Configuration loading for the provider layer.
//!
//! Reads `~/.config/ask/ask.toml` on demand.  The file is not cached — the
//! caller gets the current value on each invocation, so the user can update
//! the config without restarting the shell.

use std::path::PathBuf;

use serde::Deserialize;

use crate::ProviderError;

/// Flat configuration read from `~/.config/ask/ask.toml`.
#[derive(Debug, Deserialize)]
pub struct ProviderConfig {
    /// Which provider to use: `"ollama"` or `"openrouter"`.
    pub provider: String,
    /// Model identifier.  Format depends on the provider:
    /// - Ollama: `name:tag` (e.g. `"llama3.2"`)
    /// - OpenRouter: `provider/name` (e.g. `"anthropic/claude-3-5-haiku"`)
    pub model: String,
    /// Base URL for the Ollama server.  Defaults to `http://localhost:11434`
    /// when not specified.  Ignored for OpenRouter.
    pub base_url: Option<String>,
    /// API key for OpenRouter.  Must not be logged or exposed in error messages.
    /// Ignored for Ollama.
    pub openrouter_api_key: Option<String>,
}

impl ProviderConfig {
    /// Returns the Ollama base URL, falling back to the default.
    pub fn ollama_base_url(&self) -> &str {
        self.base_url.as_deref().unwrap_or("http://localhost:11434")
    }
}

/// Resolve the path to `~/.config/ask/ask.toml` using `$HOME`.
fn config_path() -> Result<PathBuf, ProviderError> {
    let home = std::env::var("HOME")
        .map_err(|_| ProviderError::NotConfigured("$HOME is not set".into()))?;
    Ok(PathBuf::from(home).join(".config/ask/ask.toml"))
}

/// Load and parse `~/.config/ask/ask.toml`.
///
/// Returns `ProviderError::NotConfigured` when the file does not exist or a
/// required field for the selected provider is absent.
pub fn load_config() -> Result<ProviderConfig, ProviderError> {
    let path = config_path()?;

    let raw = std::fs::read_to_string(&path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            ProviderError::NotConfigured(
                "~/.config/ask/ask.toml not found — create it with `provider` and `model` fields"
                    .to_string(),
            )
        } else {
            ProviderError::NotConfigured(format!("could not read {}: {}", path.display(), e))
        }
    })?;

    let config: ProviderConfig = toml::from_str(&raw).map_err(|e| {
        ProviderError::NotConfigured(format!("could not parse {}: {}", path.display(), e))
    })?;

    validate_config(&config)?;
    Ok(config)
}

fn validate_config(config: &ProviderConfig) -> Result<(), ProviderError> {
    match config.provider.as_str() {
        "ollama" => Ok(()),
        "openrouter" => {
            if config
                .openrouter_api_key
                .as_deref()
                .unwrap_or("")
                .is_empty()
            {
                return Err(ProviderError::NotConfigured(
                    "openrouter_api_key is required when provider = \"openrouter\"".into(),
                ));
            }
            Ok(())
        }
        other => Err(ProviderError::NotConfigured(format!(
            "unknown provider \"{other}\"; expected \"ollama\" or \"openrouter\""
        ))),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(toml: &str) -> Result<ProviderConfig, ProviderError> {
        let config: ProviderConfig =
            toml::from_str(toml).map_err(|e| ProviderError::NotConfigured(e.to_string()))?;
        validate_config(&config)?;
        Ok(config)
    }

    #[test]
    fn valid_ollama_config() {
        let cfg = parse(
            r#"
            provider = "ollama"
            model = "llama3.2"
            base_url = "http://localhost:11434"
            "#,
        )
        .unwrap();
        assert_eq!(cfg.provider, "ollama");
        assert_eq!(cfg.model, "llama3.2");
        assert_eq!(cfg.ollama_base_url(), "http://localhost:11434");
    }

    #[test]
    fn ollama_base_url_defaults() {
        let cfg = parse(
            r#"
            provider = "ollama"
            model = "llama3.2"
            "#,
        )
        .unwrap();
        assert_eq!(cfg.ollama_base_url(), "http://localhost:11434");
    }

    #[test]
    fn valid_openrouter_config() {
        let cfg = parse(
            r#"
            provider = "openrouter"
            model = "anthropic/claude-3-5-haiku"
            openrouter_api_key = "sk-or-test"
            "#,
        )
        .unwrap();
        assert_eq!(cfg.provider, "openrouter");
        assert_eq!(cfg.model, "anthropic/claude-3-5-haiku");
    }

    #[test]
    fn openrouter_missing_api_key_errors() {
        let err = parse(
            r#"
            provider = "openrouter"
            model = "anthropic/claude-3-5-haiku"
            "#,
        )
        .unwrap_err();
        match err {
            ProviderError::NotConfigured(msg) => {
                assert!(msg.contains("openrouter_api_key"), "got: {msg}");
            }
            other => panic!("expected NotConfigured, got: {other:?}"),
        }
    }

    #[test]
    fn unknown_provider_errors() {
        let err = parse(
            r#"
            provider = "groq"
            model = "llama3"
            "#,
        )
        .unwrap_err();
        match err {
            ProviderError::NotConfigured(msg) => {
                assert!(msg.contains("groq"), "got: {msg}");
            }
            other => panic!("expected NotConfigured, got: {other:?}"),
        }
    }

    #[test]
    fn missing_required_field_errors() {
        // `provider` field is required by serde; missing it is a parse error.
        let err = parse(r#"model = "llama3.2""#).unwrap_err();
        assert!(matches!(err, ProviderError::NotConfigured(_)));
    }
}
