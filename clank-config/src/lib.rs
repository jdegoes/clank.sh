use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

// ── Error type ────────────────────────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("config path unavailable: $HOME is not set")]
    NoHomePath,
    #[error("failed to read config: {0}")]
    Io(#[from] std::io::Error),
    #[error("failed to parse config: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("failed to serialize config: {0}")]
    Serialize(#[from] toml::ser::Error),
}

// ── Types ─────────────────────────────────────────────────────────────────────

/// The name of a model provider (e.g. "anthropic", "openai").
pub type ProviderName = String;

/// Configuration for a single model provider.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct ProviderConfig {
    /// The API key used to authenticate with this provider.
    pub api_key: String,
}

/// The full ask configuration, stored at `~/.config/ask/ask.toml`.
#[derive(Debug, Serialize, Deserialize, Default, PartialEq)]
pub struct AskConfig {
    /// The default model to use (e.g. "anthropic/claude-sonnet-4-5").
    pub default_model: Option<String>,
    /// Registered providers and their credentials.
    #[serde(default)]
    pub providers: HashMap<ProviderName, ProviderConfig>,
}

impl AskConfig {
    /// Register or update a provider's API key.
    pub fn add_provider(&mut self, name: ProviderName, api_key: String) {
        self.providers.insert(name, ProviderConfig { api_key });
    }

    /// Set the default model. Does not validate the model name.
    pub fn set_default_model(&mut self, model: String) {
        self.default_model = Some(model);
    }

    /// Return the API key for the provider that serves the given model,
    /// if one is configured.
    ///
    /// Model names may be in `provider/model` form or short form.
    /// This method tries to match the provider prefix, then falls back
    /// to checking if the model name itself matches a provider name.
    pub fn api_key_for_model(&self, model: &str) -> Option<&str> {
        // Try "provider/model" form first.
        if let Some((provider, _)) = model.split_once('/') {
            if let Some(config) = self.providers.get(provider) {
                return Some(&config.api_key);
            }
        }
        // Fall back: check if the model name itself is a known provider.
        if let Some(config) = self.providers.get(model) {
            return Some(&config.api_key);
        }
        // Last resort: if there's only one provider, use it.
        if self.providers.len() == 1 {
            return self.providers.values().next().map(|c| c.api_key.as_str());
        }
        None
    }
}

// ── Config path ───────────────────────────────────────────────────────────────

/// Returns the path to `~/.config/ask/ask.toml`.
///
/// Returns `None` if `$HOME` is not set (e.g. on WASM without a HOME env var).
pub fn config_path() -> Option<PathBuf> {
    std::env::var("HOME").ok().map(|home| {
        PathBuf::from(home)
            .join(".config")
            .join("ask")
            .join("ask.toml")
    })
}

// ── Load / Save ───────────────────────────────────────────────────────────────

/// Load `AskConfig` from `~/.config/ask/ask.toml`.
///
/// Returns `AskConfig::default()` if the file does not exist.
/// Returns an error if the file exists but cannot be read or parsed.
pub fn load_config() -> Result<AskConfig, ConfigError> {
    let path = config_path().ok_or(ConfigError::NoHomePath)?;

    if !path.exists() {
        return Ok(AskConfig::default());
    }

    let contents = std::fs::read_to_string(&path)?;
    let config: AskConfig = toml::from_str(&contents)?;
    Ok(config)
}

/// Save `AskConfig` to `~/.config/ask/ask.toml`.
///
/// Creates parent directories if they do not exist.
pub fn save_config(config: &AskConfig) -> Result<(), ConfigError> {
    let path = config_path().ok_or(ConfigError::NoHomePath)?;

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let contents = toml::to_string_pretty(config)?;
    std::fs::write(&path, contents)?;
    Ok(())
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    /// Temporarily override $HOME for a test, restoring it afterwards.
    fn with_temp_home<F: FnOnce(PathBuf)>(f: F) {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let old_home = env::var("HOME").ok();
        env::set_var("HOME", dir.path());
        f(dir.path().to_path_buf());
        match old_home {
            Some(h) => env::set_var("HOME", h),
            None => env::remove_var("HOME"),
        }
    }

    #[test]
    fn config_path_uses_home() {
        with_temp_home(|home| {
            let path = config_path().expect("config_path should return Some");
            assert_eq!(path, home.join(".config").join("ask").join("ask.toml"));
        });
    }

    #[test]
    fn load_config_returns_default_when_file_missing() {
        with_temp_home(|_| {
            let config = load_config().expect("should not error on missing file");
            assert_eq!(config, AskConfig::default());
        });
    }

    #[test]
    fn save_and_load_round_trip() {
        with_temp_home(|_| {
            let mut config = AskConfig::default();
            config.add_provider("anthropic".to_string(), "sk-test-key".to_string());
            config.set_default_model("anthropic/claude-sonnet-4-5".to_string());

            save_config(&config).expect("save should succeed");
            let loaded = load_config().expect("load should succeed");
            assert_eq!(loaded, config);
        });
    }

    #[test]
    fn api_key_for_model_with_provider_prefix() {
        let mut config = AskConfig::default();
        config.add_provider("anthropic".to_string(), "sk-ant-key".to_string());
        let key = config.api_key_for_model("anthropic/claude-sonnet-4-5");
        assert_eq!(key, Some("sk-ant-key"));
    }

    #[test]
    fn api_key_for_model_single_provider_fallback() {
        let mut config = AskConfig::default();
        config.add_provider("anthropic".to_string(), "sk-ant-key".to_string());
        // Model name has no prefix, but only one provider configured.
        let key = config.api_key_for_model("claude-sonnet-4-5");
        assert_eq!(key, Some("sk-ant-key"));
    }

    #[test]
    fn api_key_for_model_returns_none_when_no_providers() {
        let config = AskConfig::default();
        assert!(config
            .api_key_for_model("anthropic/claude-sonnet-4-5")
            .is_none());
    }

    #[test]
    fn add_provider_overwrites_existing_key() {
        let mut config = AskConfig::default();
        config.add_provider("anthropic".to_string(), "old-key".to_string());
        config.add_provider("anthropic".to_string(), "new-key".to_string());
        assert_eq!(config.providers["anthropic"].api_key, "new-key");
    }
}
