use std::collections::HashSet;
use std::sync::{LazyLock, RwLock};

/// Global registry of variable names marked as secret via `export --secret`.
///
/// Secret variables are available to agents via the environment but are never
/// echoed in `env`, never written to logs, never shown in `ps`, and never
/// entered into the transcript.
static SECRETS: LazyLock<RwLock<HashSet<String>>> = LazyLock::new(|| RwLock::new(HashSet::new()));

pub struct SecretsRegistry;

impl SecretsRegistry {
    /// Mark a variable name as secret.
    pub fn insert(name: impl Into<String>) {
        SECRETS
            .write()
            .expect("secrets registry poisoned")
            .insert(name.into());
    }

    /// Remove a variable from the secret registry (when unexported).
    pub fn remove(name: &str) {
        SECRETS
            .write()
            .expect("secrets registry poisoned")
            .remove(name);
    }

    /// Check if a variable name is secret.
    pub fn contains(name: &str) -> bool {
        SECRETS
            .read()
            .expect("secrets registry poisoned")
            .contains(name)
    }

    /// Return a snapshot of all secret variable names.
    pub fn snapshot() -> HashSet<String> {
        SECRETS.read().expect("secrets registry poisoned").clone()
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Unique prefix for test variable names to avoid pollution between
    /// parallel tests touching the global SECRETS registry.
    fn unique(base: &str) -> String {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(1);
        format!("{base}_{}", COUNTER.fetch_add(1, Ordering::Relaxed))
    }

    #[test]
    fn test_secrets_registry_insert_and_contains() {
        let name = unique("TEST_SECRET");
        assert!(
            !SecretsRegistry::contains(&name),
            "must not exist before insert"
        );
        SecretsRegistry::insert(&name);
        assert!(SecretsRegistry::contains(&name), "must exist after insert");
        SecretsRegistry::remove(&name); // cleanup
    }

    #[test]
    fn test_secrets_registry_remove_makes_variable_not_secret() {
        let name = unique("TEST_REMOVE");
        SecretsRegistry::insert(&name);
        assert!(SecretsRegistry::contains(&name));
        SecretsRegistry::remove(&name);
        assert!(
            !SecretsRegistry::contains(&name),
            "variable must not be secret after remove"
        );
    }

    #[test]
    fn test_secrets_registry_snapshot_reflects_current_state() {
        let name = unique("TEST_SNAPSHOT");
        SecretsRegistry::insert(&name);
        let snap = SecretsRegistry::snapshot();
        assert!(
            snap.contains(&name),
            "snapshot must include inserted variable"
        );
        SecretsRegistry::remove(&name); // cleanup
        let snap2 = SecretsRegistry::snapshot();
        assert!(
            !snap2.contains(&name),
            "snapshot after remove must not include variable"
        );
    }
}
