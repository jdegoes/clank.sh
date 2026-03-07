use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// The default model used when no explicit model is given and no default_model
/// is configured. Defined as a constant to avoid duplicating the string literal.
pub const DEFAULT_MODEL: &str = "anthropic/claude-sonnet-4-5";

/// Configuration for the `ask` command.
/// Loaded from `~/.config/ask/ask.toml`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AskConfig {
    /// The default model to use, e.g. `"anthropic/claude-sonnet-4-5"`.
    #[serde(default)]
    pub default_model: Option<String>,

    /// Provider configurations, keyed by provider name (e.g. `"anthropic"`).
    #[serde(default)]
    pub providers: HashMap<String, ProviderConfig>,
}

/// Configuration for a single model provider.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProviderConfig {
    /// API key for this provider.
    pub api_key: Option<String>,

    /// Base URL for this provider (required for local providers such as Ollama
    /// and OpenAI-compatible servers; optional for cloud providers).
    #[serde(default)]
    pub base_url: Option<String>,
}

/// Errors that can occur when loading or saving configuration.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error(
        "config file not found at {path}: run `model add <provider> --key <key>` to configure"
    )]
    NotFound { path: PathBuf },

    #[error("failed to read config at {path}: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("failed to parse config at {path}: {source}")]
    Parse {
        path: PathBuf,
        source: toml::de::Error,
    },

    #[error("failed to write config to {path}: {source}")]
    Write {
        path: PathBuf,
        source: std::io::Error,
    },
}

impl AskConfig {
    /// Load configuration from `~/.config/ask/ask.toml`.
    pub fn load() -> Result<Self, ConfigError> {
        let path = config_path();
        let contents = std::fs::read_to_string(&path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                ConfigError::NotFound { path: path.clone() }
            } else {
                ConfigError::Read {
                    path: path.clone(),
                    source: e,
                }
            }
        })?;
        toml::from_str(&contents).map_err(|e| ConfigError::Parse {
            path: path.clone(),
            source: e,
        })
    }

    /// Load configuration, returning a default empty config if the file
    /// does not exist. Returns an error only for read/parse failures.
    pub fn load_or_default() -> Result<Self, ConfigError> {
        match Self::load() {
            Ok(c) => Ok(c),
            Err(ConfigError::NotFound { .. }) => Ok(Self::default()),
            Err(e) => Err(e),
        }
    }

    /// Save configuration atomically to the config file path.
    ///
    /// The file is written to a `.tmp` sibling in the same directory, then
    /// renamed into place. This ensures the rename is within the same
    /// filesystem, making it atomic on all supported platforms.
    ///
    /// The parent directory is created if it does not exist.
    pub fn save(&self) -> Result<(), ConfigError> {
        let path = config_path();
        let parent = path.parent().unwrap_or_else(|| std::path::Path::new("."));

        std::fs::create_dir_all(parent).map_err(|e| ConfigError::Write {
            path: parent.to_path_buf(),
            source: e,
        })?;

        let contents = toml::to_string_pretty(self).map_err(|e| ConfigError::Write {
            path: path.clone(),
            source: std::io::Error::other(e.to_string()),
        })?;

        let tmp_path = path.with_extension("toml.tmp");

        std::fs::write(&tmp_path, &contents).map_err(|e| ConfigError::Write {
            path: tmp_path.clone(),
            source: e,
        })?;

        std::fs::rename(&tmp_path, &path).map_err(|e| ConfigError::Write {
            path: path.clone(),
            source: e,
        })?;

        Ok(())
    }

    /// Resolve the effective model name, preferring the explicit override,
    /// then the config default, then a hardcoded fallback.
    pub fn resolve_model(&self, explicit: Option<&str>) -> String {
        explicit
            .or(self.default_model.as_deref())
            .unwrap_or(DEFAULT_MODEL)
            .to_string()
    }

    /// Get the API key for a provider.
    pub fn api_key(&self, provider: &str) -> Option<&str> {
        self.providers.get(provider)?.api_key.as_deref()
    }

    /// Get the configured base URL for a provider, if any.
    pub fn base_url(&self, provider: &str) -> Option<&str> {
        self.providers.get(provider)?.base_url.as_deref()
    }
}

/// Returns the path to the ask config file.
///
/// If the `CLANK_CONFIG` environment variable is set to a non-empty string,
/// that path is used. Otherwise the platform default is used:
/// - Linux:  `~/.config/ask/ask.toml`
/// - macOS:  `~/Library/Application Support/ask/ask.toml`
///
/// # Example
///
/// ```sh
/// CLANK_CONFIG=./ask.toml clank
/// ```
pub fn config_path() -> PathBuf {
    match std::env::var("CLANK_CONFIG") {
        Ok(val) if !val.is_empty() => PathBuf::from(val),
        _ => dirs_next::config_dir()
            .unwrap_or_else(|| PathBuf::from("~/.config"))
            .join("ask")
            .join("ask.toml"),
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A process-wide mutex that serialises every test touching `CLANK_CONFIG`.
    ///
    /// `std::env::set_var` is not thread-safe when other threads are concurrently
    /// reading the environment. Holding this lock for the duration of any test that
    /// calls `set_var`/`remove_var` prevents data races between parallel test threads.
    pub(crate) static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn test_ask_config_load_valid() {
        let toml = r#"
            default_model = "anthropic/claude-sonnet-4-5"
            [providers.anthropic]
            api_key = "sk-ant-test"
        "#;
        let config: AskConfig = toml::from_str(toml).expect("parse failed");
        assert_eq!(
            config.default_model.as_deref(),
            Some("anthropic/claude-sonnet-4-5")
        );
        assert_eq!(config.api_key("anthropic"), Some("sk-ant-test"));
    }

    #[test]
    fn test_ask_config_load_returns_not_found_for_missing_file() {
        let _lock = ENV_LOCK.lock().unwrap();
        std::env::set_var("CLANK_CONFIG", "/nonexistent/path/ask.toml");
        let result = AskConfig::load();
        std::env::remove_var("CLANK_CONFIG");
        assert!(
            matches!(result, Err(ConfigError::NotFound { .. })),
            "expected ConfigError::NotFound, got: {result:?}"
        );
    }

    #[test]
    fn test_ask_config_resolve_model_explicit_wins() {
        let config = AskConfig {
            default_model: Some("anthropic/claude-haiku".to_string()),
            ..Default::default()
        };
        assert_eq!(config.resolve_model(Some("openai/gpt-4o")), "openai/gpt-4o");
    }

    #[test]
    fn test_ask_config_resolve_model_uses_default() {
        let config = AskConfig {
            default_model: Some("anthropic/claude-haiku".to_string()),
            ..Default::default()
        };
        assert_eq!(config.resolve_model(None), "anthropic/claude-haiku");
    }

    #[test]
    fn test_ask_config_resolve_model_fallback() {
        let config = AskConfig::default();
        assert_eq!(config.resolve_model(None), DEFAULT_MODEL);
    }

    #[test]
    fn test_config_path_uses_env_var_when_set() {
        let _lock = ENV_LOCK.lock().unwrap();
        let test_path = "/tmp/clank-test-ask.toml";
        std::env::set_var("CLANK_CONFIG", test_path);
        let path = config_path();
        std::env::remove_var("CLANK_CONFIG");
        assert_eq!(path, PathBuf::from(test_path));
    }

    #[test]
    fn test_config_path_uses_default_when_env_unset() {
        let _lock = ENV_LOCK.lock().unwrap();
        std::env::remove_var("CLANK_CONFIG");
        let path = config_path();
        assert!(
            path.ends_with("ask/ask.toml"),
            "expected platform default path, got: {path:?}"
        );
    }

    #[test]
    fn test_config_path_uses_default_when_env_empty() {
        let _lock = ENV_LOCK.lock().unwrap();
        std::env::set_var("CLANK_CONFIG", "");
        let path = config_path();
        std::env::remove_var("CLANK_CONFIG");
        assert!(
            path.ends_with("ask/ask.toml"),
            "empty CLANK_CONFIG should use platform default, got: {path:?}"
        );
    }

    #[test]
    fn test_ask_loads_config_from_env_var() {
        use std::io::Write;
        let _lock = ENV_LOCK.lock().unwrap();
        let mut tmp = tempfile::NamedTempFile::new().expect("tempfile");
        write!(
            tmp,
            r#"
            default_model = "openrouter/test-model"
            [providers.openrouter]
            api_key = "sk-or-env-test"
            "#
        )
        .unwrap();

        std::env::set_var("CLANK_CONFIG", tmp.path().to_str().unwrap());
        let config = AskConfig::load().expect("should load from env var path");
        std::env::remove_var("CLANK_CONFIG");

        assert_eq!(config.api_key("openrouter"), Some("sk-or-env-test"));
        assert_eq!(
            config.default_model.as_deref(),
            Some("openrouter/test-model")
        );
    }

    // -----------------------------------------------------------------------
    // base_url field tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_provider_config_base_url_roundtrips() {
        let toml = r#"
            [providers.ollama]
            base_url = "http://localhost:11434"
        "#;
        let config: AskConfig = toml::from_str(toml).expect("parse failed");
        assert_eq!(config.base_url("ollama"), Some("http://localhost:11434"));
        // Round-trip: serialise and deserialise again.
        let serialised = toml::to_string_pretty(&config).expect("serialise");
        let config2: AskConfig = toml::from_str(&serialised).expect("re-parse");
        assert_eq!(config2.base_url("ollama"), Some("http://localhost:11434"));
    }

    #[test]
    fn test_provider_config_no_base_url_deserialises() {
        // Existing TOML with no base_url field must parse without error.
        let toml = r#"
            [providers.anthropic]
            api_key = "sk-ant-test"
        "#;
        let config: AskConfig = toml::from_str(toml).expect("parse failed");
        assert_eq!(config.base_url("anthropic"), None);
        assert_eq!(config.api_key("anthropic"), Some("sk-ant-test"));
    }

    #[test]
    fn test_ask_config_base_url_helper() {
        let toml = r#"
            [providers.ollama]
            base_url = "http://myhost:11434"
        "#;
        let config: AskConfig = toml::from_str(toml).expect("parse failed");
        assert_eq!(config.base_url("ollama"), Some("http://myhost:11434"));
        assert_eq!(config.base_url("anthropic"), None);
    }

    #[test]
    fn test_ask_config_save_preserves_unrelated_fields() {
        // A config with both api_key and base_url on a provider must round-trip both.
        let toml = r#"
            default_model = "openai-compat/phi4"
            [providers.openai-compat]
            api_key = "sk-x"
            base_url = "http://localhost:8080"
        "#;
        let config: AskConfig = toml::from_str(toml).expect("parse failed");
        assert_eq!(config.api_key("openai-compat"), Some("sk-x"));
        assert_eq!(
            config.base_url("openai-compat"),
            Some("http://localhost:8080")
        );
        let serialised = toml::to_string_pretty(&config).expect("serialise");
        let config2: AskConfig = toml::from_str(&serialised).expect("re-parse");
        assert_eq!(config2.api_key("openai-compat"), Some("sk-x"));
        assert_eq!(
            config2.base_url("openai-compat"),
            Some("http://localhost:8080")
        );
    }

    // -----------------------------------------------------------------------
    // save() tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_ask_config_save_roundtrip() {
        let _lock = ENV_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("ask.toml");
        std::env::set_var("CLANK_CONFIG", path.to_str().unwrap());

        let mut providers = std::collections::HashMap::new();
        providers.insert(
            "ollama".to_string(),
            ProviderConfig {
                api_key: None,
                base_url: Some("http://localhost:11434".to_string()),
            },
        );
        let config = AskConfig {
            default_model: Some("ollama/llama3.2".to_string()),
            providers,
        };

        config.save().expect("save failed");
        let loaded = AskConfig::load().expect("load failed");
        std::env::remove_var("CLANK_CONFIG");

        assert_eq!(loaded.default_model.as_deref(), Some("ollama/llama3.2"));
        assert_eq!(loaded.base_url("ollama"), Some("http://localhost:11434"));
    }

    #[test]
    fn test_ask_config_save_creates_parent_dir() {
        let _lock = ENV_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("nested").join("dir").join("ask.toml");
        std::env::set_var("CLANK_CONFIG", path.to_str().unwrap());

        let config = AskConfig::default();
        config.save().expect("save should create parent dirs");

        assert!(path.exists(), "config file should exist after save");
        std::env::remove_var("CLANK_CONFIG");
    }

    #[test]
    fn test_ask_config_save_atomic_temp_in_same_dir() {
        let _lock = ENV_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("ask.toml");
        std::env::set_var("CLANK_CONFIG", path.to_str().unwrap());

        let config = AskConfig::default();
        config.save().expect("save failed");

        // After a successful save the .tmp file must be gone (renamed away).
        let tmp_path = dir.path().join("ask.toml.tmp");
        assert!(
            !tmp_path.exists(),
            ".tmp file should be cleaned up after atomic rename"
        );
        std::env::remove_var("CLANK_CONFIG");
    }
}
