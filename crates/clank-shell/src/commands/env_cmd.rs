use std::fmt::Write as _;

use async_trait::async_trait;

use crate::process::{Process, ProcessContext, ProcessResult};
use crate::secrets::SecretsRegistry;

pub struct EnvProcess;

#[async_trait]
impl Process for EnvProcess {
    async fn run(&self, mut ctx: ProcessContext) -> ProcessResult {
        let secrets = SecretsRegistry::snapshot();
        let mut out = String::new();
        for (key, value) in &ctx.env {
            if secrets.contains(key.as_str()) {
                let _ = writeln!(out, "{key}=***");
            } else {
                let _ = writeln!(out, "{key}={value}");
            }
        }
        let _ = ctx.io.write_stdout(out.as_bytes());
        ProcessResult::success()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::process::ProcessIo;
    use brush_core::openfiles::OpenFile;
    use std::collections::HashMap;

    async fn run(env: HashMap<String, String>) -> (String, i32) {
        let out = tempfile::NamedTempFile::new().unwrap();
        let ctx = ProcessContext {
            argv: vec!["env".to_string()],
            env,
            io: ProcessIo {
                stdin: OpenFile::Stdin(std::io::stdin()),
                stdout: OpenFile::from(out.reopen().unwrap()),
                stderr: OpenFile::Stderr(std::io::stderr()),
            },
            pid: 0,
            cwd: std::path::PathBuf::from("/"),
        };
        let result = EnvProcess.run(ctx).await;
        (
            std::fs::read_to_string(out.path()).unwrap(),
            result.exit_code,
        )
    }

    #[tokio::test]
    async fn test_env_masks_secret_variables() {
        SecretsRegistry::insert("MY_SECRET");
        let mut env = HashMap::new();
        env.insert("MY_SECRET".to_string(), "super-secret-value".to_string());
        let (stdout, code) = run(env).await;

        // Clean up before asserting — a failing assert must not leave the
        // secret registered, which would pollute subsequent tests.
        SecretsRegistry::remove("MY_SECRET");

        assert_eq!(code, 0);
        assert!(
            stdout.contains("MY_SECRET=***"),
            "secret must be masked: {stdout}"
        );
        assert!(
            !stdout.contains("super-secret-value"),
            "plaintext secret must not appear: {stdout}"
        );
    }

    #[tokio::test]
    async fn test_env_prints_non_secret_variables_plaintext() {
        let mut env = HashMap::new();
        env.insert("MY_VAR".to_string(), "my_value".to_string());
        let (stdout, code) = run(env).await;
        assert_eq!(code, 0);
        assert!(
            stdout.contains("MY_VAR=my_value"),
            "variable must appear plaintext: {stdout}"
        );
    }

    #[tokio::test]
    async fn test_env_empty_env_produces_no_output() {
        let (stdout, code) = run(HashMap::new()).await;
        assert_eq!(code, 0);
        assert!(
            stdout.is_empty(),
            "empty env must produce no output: {stdout}"
        );
    }
}
