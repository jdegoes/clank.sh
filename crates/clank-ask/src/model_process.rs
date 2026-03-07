use std::fmt::Write as _;

use crate::config::{AskConfig, ProviderConfig};

/// The result of a `model` subcommand invocation.
#[derive(Debug)]
pub struct ModelOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

impl ModelOutput {
    fn ok(stdout: impl Into<String>) -> Self {
        Self {
            stdout: stdout.into(),
            stderr: String::new(),
            exit_code: 0,
        }
    }

    fn err(stderr: impl Into<String>, exit_code: i32) -> Self {
        Self {
            stdout: String::new(),
            stderr: stderr.into(),
            exit_code,
        }
    }
}

/// Run the model command logic.
pub fn run_model(argv: &[String]) -> ModelOutput {
    let subcommand = argv.get(1).map(String::as_str).unwrap_or("");

    match subcommand {
        "list" => run_model_list(),
        "add" => run_model_add(&argv[2..]),
        "default" => run_model_default(&argv[2..]),
        "remove" | "info" => ModelOutput::err(
            format!("clank: model {subcommand}: not yet implemented (planned)\n"),
            2,
        ),
        "" => ModelOutput::err("usage: model <list|add|remove|default|info>\n", 2),
        other => ModelOutput::err(format!("clank: model: unknown subcommand: {other}\n"), 2),
    }
}

// ---------------------------------------------------------------------------
// model list
// ---------------------------------------------------------------------------

fn run_model_list() -> ModelOutput {
    let config = AskConfig::load_or_default().unwrap_or_default();
    if config.providers.is_empty() {
        return ModelOutput::ok(
            "No providers configured.\n\
             Run: model add anthropic --key <KEY>\n",
        );
    }

    let mut out = String::new();
    let default_model = config
        .default_model
        .as_deref()
        .unwrap_or(crate::config::DEFAULT_MODEL);
    let _ = write!(out, "Default model: {default_model}\n\nProviders:\n");

    let mut names: Vec<_> = config.providers.keys().collect();
    names.sort();

    for name in names {
        let p = &config.providers[name];
        let has_key = p.api_key.as_deref().is_some_and(|k| !k.is_empty());
        let has_url = p.base_url.as_deref().is_some_and(|u| !u.is_empty());

        let url = p.base_url.as_deref().unwrap_or("");
        let status = match (has_url, has_key) {
            (true, true) => format!("base_url={url}, api_key configured"),
            (true, false) => format!("base_url={url}"),
            (false, true) => "api_key configured".to_string(),
            (false, false) => "no configuration".to_string(),
        };

        let _ = writeln!(out, "  {name}: {status}");
    }

    ModelOutput::ok(out)
}

// ---------------------------------------------------------------------------
// model default
// ---------------------------------------------------------------------------

fn run_model_default(args: &[String]) -> ModelOutput {
    match args.first().map(String::as_str) {
        // No argument: print the currently configured default model.
        None => {
            let config = match AskConfig::load_or_default() {
                Ok(c) => c,
                Err(e) => {
                    return ModelOutput::err(format!("clank: model default: {e}\n"), 1);
                }
            };
            let model = config
                .default_model
                .as_deref()
                .unwrap_or(crate::config::DEFAULT_MODEL);
            ModelOutput::ok(format!("{model}\n"))
        }
        // Argument: set the default model.
        Some(model) => {
            let mut config = match AskConfig::load_or_default() {
                Ok(c) => c,
                Err(e) => {
                    return ModelOutput::err(format!("clank: model default: {e}\n"), 1);
                }
            };
            config.default_model = Some(model.to_string());
            if let Err(e) = config.save() {
                return ModelOutput::err(format!("clank: model default: {e}\n"), 1);
            }
            ModelOutput::ok(format!("Default model set to '{model}'.\n"))
        }
    }
}

// ---------------------------------------------------------------------------
// model add
// ---------------------------------------------------------------------------

fn run_model_add(args: &[String]) -> ModelOutput {
    // First positional argument is the provider name.
    let provider = match args.first() {
        Some(p) if !p.starts_with('-') => p.as_str(),
        _ => {
            return ModelOutput::err(
                "clank: model add: provider name required\n\
                 usage: model add <provider> [--key <key>] [--url <url>]\n",
                2,
            );
        }
    };

    // Parse flags from the remainder.
    let mut new_key: Option<String> = None;
    let mut new_url: Option<String> = None;
    let mut i = 1usize;
    while i < args.len() {
        match args[i].as_str() {
            "--key" => {
                i += 1;
                match args.get(i) {
                    Some(v) => new_key = Some(v.clone()),
                    None => {
                        return ModelOutput::err("clank: model add: --key requires a value\n", 2);
                    }
                }
            }
            "--url" => {
                i += 1;
                match args.get(i) {
                    Some(v) => new_url = Some(v.clone()),
                    None => {
                        return ModelOutput::err("clank: model add: --url requires a value\n", 2);
                    }
                }
            }
            unknown => {
                return ModelOutput::err(
                    format!(
                        "clank: model add: unknown flag: {unknown}\n\
                         usage: model add <provider> [--key <key>] [--url <url>]\n"
                    ),
                    2,
                );
            }
        }
        i += 1;
    }

    // Validate required fields per provider.
    match provider {
        "anthropic" | "openrouter" => {
            if new_key.is_none() {
                return ModelOutput::err(
                    format!(
                        "clank: model add: --key is required for provider '{provider}'\n\
                         usage: model add {provider} --key <API_KEY>\n"
                    ),
                    2,
                );
            }
        }
        "openai-compat" => {
            if new_url.is_none() {
                return ModelOutput::err(
                    "clank: model add: --url is required for provider 'openai-compat'\n\
                     usage: model add openai-compat --url <BASE_URL> [--key <API_KEY>]\n",
                    2,
                );
            }
        }
        _ => {} // ollama and unknown providers: no required fields
    }

    // Load, update (merge — only write supplied fields), and save.
    let mut config = match AskConfig::load_or_default() {
        Ok(c) => c,
        Err(e) => {
            return ModelOutput::err(format!("clank: model add: {e}\n"), 1);
        }
    };

    let entry: &mut ProviderConfig = config.providers.entry(provider.to_string()).or_default();

    if let Some(key) = new_key {
        entry.api_key = Some(key);
    }
    if let Some(url) = new_url {
        entry.base_url = Some(url);
    }

    // For ollama, set the default base_url if none was supplied and none is
    // already present.
    if provider == "ollama" && entry.base_url.is_none() {
        entry.base_url = Some("http://localhost:11434".to_string());
    }

    if let Err(e) = config.save() {
        return ModelOutput::err(format!("clank: model add: {e}\n"), 1);
    }

    ModelOutput::ok(format!("Provider '{provider}' configured.\n"))
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn with_temp_config<F: FnOnce() -> R, R>(f: F) -> R {
        // Serialise all env-var-touching tests via the shared lock.
        let _lock = crate::config::tests::ENV_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("ask.toml");
        std::env::set_var("CLANK_CONFIG", path.to_str().unwrap());
        let result = f();
        std::env::remove_var("CLANK_CONFIG");
        result
    }

    #[test]
    fn test_model_list_no_config() {
        with_temp_config(|| {
            // With an isolated empty config, model list must report no providers.
            let ModelOutput {
                stdout: out,
                exit_code: code,
                ..
            } = run_model(&["model".into(), "list".into()]);
            assert_eq!(code, 0);
            assert!(
                out.contains("No providers configured."),
                "expected 'No providers configured.' but got: {out}"
            );
        });
    }

    #[test]
    fn test_model_no_subcommand_exits_2() {
        let ModelOutput {
            stderr: err,
            exit_code: code,
            ..
        } = run_model(&["model".into()]);
        assert_eq!(code, 2);
        assert!(err.contains("usage"));
    }

    #[test]
    fn test_model_unknown_subcommand_exits_2() {
        let ModelOutput {
            stderr: err,
            exit_code: code,
            ..
        } = run_model(&["model".into(), "frobnicate".into()]);
        assert_eq!(code, 2);
        assert!(err.contains("unknown subcommand"));
    }

    #[test]
    fn test_model_remove_stub_exits_2() {
        let ModelOutput {
            stderr: err,
            exit_code: code,
            ..
        } = run_model(&["model".into(), "remove".into()]);
        assert_eq!(
            code, 2,
            "unimplemented subcommand must exit 2 (bad arguments)"
        );
        assert!(err.contains("not yet implemented"));
    }

    #[test]
    fn test_model_info_stub_exits_2() {
        let ModelOutput {
            stderr: err,
            exit_code: code,
            ..
        } = run_model(&["model".into(), "info".into()]);
        assert_eq!(
            code, 2,
            "unimplemented subcommand must exit 2 (bad arguments)"
        );
        assert!(err.contains("not yet implemented"));
    }

    // -----------------------------------------------------------------------
    // model add tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_model_add_no_provider_exits_2() {
        let ModelOutput {
            stderr: err,
            exit_code: code,
            ..
        } = run_model(&["model".into(), "add".into()]);
        assert_eq!(code, 2);
        assert!(err.contains("provider name required"), "got: {err}");
    }

    #[test]
    fn test_model_add_unknown_flag_exits_2() {
        let ModelOutput {
            stderr: err,
            exit_code: code,
            ..
        } = run_model(&[
            "model".into(),
            "add".into(),
            "ollama".into(),
            "--frobnicate".into(),
        ]);
        assert_eq!(code, 2);
        assert!(err.contains("unknown flag"), "got: {err}");
    }

    #[test]
    fn test_model_add_anthropic_no_key_exits_2() {
        let ModelOutput {
            stderr: err,
            exit_code: code,
            ..
        } = run_model(&["model".into(), "add".into(), "anthropic".into()]);
        assert_eq!(code, 2);
        assert!(err.contains("--key is required"), "got: {err}");
    }

    #[test]
    fn test_model_add_openrouter_no_key_exits_2() {
        let ModelOutput {
            stderr: err,
            exit_code: code,
            ..
        } = run_model(&["model".into(), "add".into(), "openrouter".into()]);
        assert_eq!(code, 2);
        assert!(err.contains("--key is required"), "got: {err}");
    }

    #[test]
    fn test_model_add_openai_compat_no_url_exits_2() {
        let ModelOutput {
            stderr: err,
            exit_code: code,
            ..
        } = run_model(&["model".into(), "add".into(), "openai-compat".into()]);
        assert_eq!(code, 2);
        assert!(err.contains("--url is required"), "got: {err}");
    }

    #[test]
    fn test_model_add_ollama_no_url_flag_writes_default() {
        with_temp_config(|| {
            let ModelOutput {
                stdout: out,
                stderr: err,
                exit_code: code,
            } = run_model(&["model".into(), "add".into(), "ollama".into()]);
            assert_eq!(code, 0, "stderr: {err}");
            assert!(out.contains("configured"), "got: {out}");

            let config = AskConfig::load().expect("config must exist after add");
            assert_eq!(
                config.base_url("ollama"),
                Some("http://localhost:11434"),
                "default ollama URL must be written"
            );
        });
    }

    #[test]
    fn test_model_add_ollama_custom_url() {
        with_temp_config(|| {
            let ModelOutput {
                stderr: err,
                exit_code: code,
                ..
            } = run_model(&[
                "model".into(),
                "add".into(),
                "ollama".into(),
                "--url".into(),
                "http://remote:11434".into(),
            ]);
            assert_eq!(code, 0, "stderr: {err}");

            let config = AskConfig::load().expect("config must exist");
            assert_eq!(config.base_url("ollama"), Some("http://remote:11434"));
        });
    }

    #[test]
    fn test_model_add_ollama_preserves_existing_api_key() {
        with_temp_config(|| {
            // Pre-populate an api_key for ollama.
            let mut config = AskConfig::default();
            config.providers.insert(
                "ollama".to_string(),
                ProviderConfig {
                    api_key: Some("existing-key".to_string()),
                    base_url: None,
                },
            );
            config.save().expect("pre-save");

            // Now run model add ollama --url, which should only update base_url.
            run_model(&[
                "model".into(),
                "add".into(),
                "ollama".into(),
                "--url".into(),
                "http://x:11434".into(),
            ]);

            let loaded = AskConfig::load().expect("load");
            assert_eq!(
                loaded.api_key("ollama"),
                Some("existing-key"),
                "api_key must be preserved"
            );
            assert_eq!(loaded.base_url("ollama"), Some("http://x:11434"));
        });
    }

    #[test]
    fn test_model_add_anthropic_writes_key() {
        with_temp_config(|| {
            let ModelOutput {
                stderr: err,
                exit_code: code,
                ..
            } = run_model(&[
                "model".into(),
                "add".into(),
                "anthropic".into(),
                "--key".into(),
                "sk-ant-x".into(),
            ]);
            assert_eq!(code, 0, "stderr: {err}");

            let config = AskConfig::load().expect("config must exist");
            assert_eq!(config.api_key("anthropic"), Some("sk-ant-x"));
            assert_eq!(config.base_url("anthropic"), None);
        });
    }

    #[test]
    fn test_model_add_anthropic_preserves_existing_base_url() {
        with_temp_config(|| {
            // Pre-populate a base_url for anthropic (unusual but possible).
            let mut config = AskConfig::default();
            config.providers.insert(
                "anthropic".to_string(),
                ProviderConfig {
                    api_key: None,
                    base_url: Some("http://proxy:443".to_string()),
                },
            );
            config.save().expect("pre-save");

            run_model(&[
                "model".into(),
                "add".into(),
                "anthropic".into(),
                "--key".into(),
                "sk-ant-x".into(),
            ]);

            let loaded = AskConfig::load().expect("load");
            assert_eq!(loaded.api_key("anthropic"), Some("sk-ant-x"));
            assert_eq!(
                loaded.base_url("anthropic"),
                Some("http://proxy:443"),
                "base_url must be preserved"
            );
        });
    }

    #[test]
    fn test_model_add_openai_compat_with_url() {
        with_temp_config(|| {
            let ModelOutput {
                stderr: err,
                exit_code: code,
                ..
            } = run_model(&[
                "model".into(),
                "add".into(),
                "openai-compat".into(),
                "--url".into(),
                "http://localhost:8080".into(),
            ]);
            assert_eq!(code, 0, "stderr: {err}");

            let config = AskConfig::load().expect("config must exist");
            assert_eq!(
                config.base_url("openai-compat"),
                Some("http://localhost:8080")
            );
            assert_eq!(config.api_key("openai-compat"), None);
        });
    }

    #[test]
    fn test_model_add_openai_compat_with_url_and_key() {
        with_temp_config(|| {
            let ModelOutput {
                stderr: err,
                exit_code: code,
                ..
            } = run_model(&[
                "model".into(),
                "add".into(),
                "openai-compat".into(),
                "--url".into(),
                "http://localhost:8080".into(),
                "--key".into(),
                "sk-local".into(),
            ]);
            assert_eq!(code, 0, "stderr: {err}");

            let config = AskConfig::load().expect("config must exist");
            assert_eq!(
                config.base_url("openai-compat"),
                Some("http://localhost:8080")
            );
            assert_eq!(config.api_key("openai-compat"), Some("sk-local"));
        });
    }

    // -----------------------------------------------------------------------
    // model list display tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_model_list_shows_key_only() {
        with_temp_config(|| {
            run_model(&[
                "model".into(),
                "add".into(),
                "anthropic".into(),
                "--key".into(),
                "sk-ant-x".into(),
            ]);
            let ModelOutput {
                stdout: out,
                exit_code: code,
                ..
            } = run_model(&["model".into(), "list".into()]);
            assert_eq!(code, 0);
            assert!(out.contains("anthropic: api_key configured"), "got: {out}");
        });
    }

    #[test]
    fn test_model_list_shows_base_url_only() {
        with_temp_config(|| {
            run_model(&["model".into(), "add".into(), "ollama".into()]);
            let ModelOutput {
                stdout: out,
                exit_code: code,
                ..
            } = run_model(&["model".into(), "list".into()]);
            assert_eq!(code, 0);
            assert!(
                out.contains("ollama: base_url=http://localhost:11434"),
                "got: {out}"
            );
        });
    }

    #[test]
    fn test_model_list_shows_both_fields() {
        with_temp_config(|| {
            run_model(&[
                "model".into(),
                "add".into(),
                "openai-compat".into(),
                "--url".into(),
                "http://localhost:8080".into(),
                "--key".into(),
                "sk-x".into(),
            ]);
            let ModelOutput {
                stdout: out,
                exit_code: code,
                ..
            } = run_model(&["model".into(), "list".into()]);
            assert_eq!(code, 0);
            assert!(
                out.contains("openai-compat: base_url=http://localhost:8080, api_key configured"),
                "got: {out}"
            );
        });
    }

    // -----------------------------------------------------------------------
    // model default tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_model_default_no_arg_prints_builtin_default() {
        with_temp_config(|| {
            // No config file → must print DEFAULT_MODEL constant.
            let ModelOutput {
                stdout: out,
                stderr: err,
                exit_code: code,
            } = run_model(&["model".into(), "default".into()]);
            assert_eq!(code, 0, "stderr: {err}");
            assert!(
                out.contains(crate::config::DEFAULT_MODEL),
                "expected DEFAULT_MODEL in output, got: {out}"
            );
        });
    }

    #[test]
    fn test_model_default_no_arg_prints_configured_default() {
        with_temp_config(|| {
            // Set a custom default first, then query.
            run_model(&[
                "model".into(),
                "default".into(),
                "anthropic/claude-haiku-3-5".into(),
            ]);
            let ModelOutput {
                stdout: out,
                stderr: err,
                exit_code: code,
            } = run_model(&["model".into(), "default".into()]);
            assert_eq!(code, 0, "stderr: {err}");
            assert!(
                out.contains("anthropic/claude-haiku-3-5"),
                "expected configured default in output, got: {out}"
            );
        });
    }

    #[test]
    fn test_model_default_sets_model() {
        with_temp_config(|| {
            let ModelOutput {
                stdout: out,
                stderr: err,
                exit_code: code,
            } = run_model(&["model".into(), "default".into(), "openai/gpt-4o".into()]);
            assert_eq!(code, 0, "stderr: {err}");
            assert!(out.contains("openai/gpt-4o"), "confirmation missing: {out}");

            // Verify it was persisted.
            let config = crate::config::AskConfig::load().expect("config must exist");
            assert_eq!(
                config.default_model.as_deref(),
                Some("openai/gpt-4o"),
                "default_model not persisted"
            );
        });
    }

    #[test]
    fn test_model_default_does_not_clobber_providers() {
        with_temp_config(|| {
            // Add a provider first.
            run_model(&[
                "model".into(),
                "add".into(),
                "anthropic".into(),
                "--key".into(),
                "sk-ant-x".into(),
            ]);
            // Now change the default model.
            run_model(&[
                "model".into(),
                "default".into(),
                "anthropic/claude-haiku-3-5".into(),
            ]);
            // Provider must still be present.
            let config = crate::config::AskConfig::load().expect("config must exist");
            assert_eq!(
                config.api_key("anthropic"),
                Some("sk-ant-x"),
                "provider must not be clobbered by model default"
            );
            assert_eq!(
                config.default_model.as_deref(),
                Some("anthropic/claude-haiku-3-5")
            );
        });
    }
}
