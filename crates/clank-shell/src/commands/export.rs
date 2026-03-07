use async_trait::async_trait;

use crate::process::{Process, ProcessContext, ProcessResult};
use crate::secrets::SecretsRegistry;

/// Replacement for Brush's native `export` builtin that adds `--secret` support.
///
/// All standard `export` behaviour (set/unset, `-f`, `-n`, `-p`, assignments)
/// is delegated to the shell's environment via `ProcessContext::env`. The
/// `--secret` flag additionally marks the variable name in `SecretsRegistry`.
///
/// Note: Because Brush handles `export` as a declaration builtin (it processes
/// assignment-style args like `KEY=value`), we register this with
/// `declaration_builtin: true` in `clank_builtins()`.
pub struct ExportProcess;

#[async_trait]
impl Process for ExportProcess {
    async fn run(&self, ctx: ProcessContext) -> ProcessResult {
        let secret = ctx.argv.iter().any(|a| a == "--secret");

        // Collect assignment args (KEY=value or just KEY).
        let decls: Vec<&String> = ctx
            .argv
            .iter()
            .skip(1)
            .filter(|a| a != &"--secret" && !a.starts_with('-'))
            .collect();

        for decl in decls {
            let name = if let Some((k, _)) = decl.split_once('=') {
                k
            } else {
                decl.as_str()
            };

            if secret {
                SecretsRegistry::insert(name);
            }
        }

        // The actual export semantics (setting the export flag on shell variables)
        // are handled by Brush's declaration builtin machinery before our
        // execute_func is called — we only need to handle the --secret side effect.
        ProcessResult::success()
    }
}
