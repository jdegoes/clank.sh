//! clank-ask — implementation of the `ask` command.
//!
//! `ask` is registered as a brush-core builtin with `Subprocess` scope.  It
//! reads the current transcript as context (unless `--fresh` is given), accepts
//! a prompt from positional arguments and/or piped stdin, calls the configured
//! LLM provider, prints the response to stdout, and records the response as an
//! `AiResponse` transcript entry.

use std::io::{IsTerminal as _, Read as _, Write as _};
use std::sync::{Arc, Mutex, OnceLock};

use brush_core::builtins::{simple_builtin, ContentType, Registration, SimpleCommand};
use brush_core::{commands::ExecutionContext, results::ExecutionResult};

use clank_http::NativeHttpClient;
use clank_provider::{provider_from_config, Message, ProviderError, Role};

// ---------------------------------------------------------------------------
// Pending response cell
//
// `ask` stores the model response here after a successful call.  `run_statement`
// in clank-core reads it via `take_pending_response()` and records the
// `AiResponse` entry *after* the `Command` entry, maintaining correct ordering.
// ---------------------------------------------------------------------------

static PENDING_RESPONSE: OnceLock<Mutex<Option<String>>> = OnceLock::new();

fn pending_response() -> &'static Mutex<Option<String>> {
    PENDING_RESPONSE.get_or_init(|| Mutex::new(None))
}

/// Take the pending `AiResponse` text set by the last successful `ask`
/// invocation, if any.  Clears the cell on read.
///
/// Called by `clank_core::run_statement` after execution so the `AiResponse`
/// entry is recorded after the `Command` entry.
pub fn take_pending_response() -> Option<String> {
    pending_response()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .take()
}

const SYSTEM_PROMPT: &str = "\
You are an AI assistant integrated into a Unix shell called clank.sh. \
The user's shell session transcript is provided as context. \
Answer the user's question or complete their request concisely and accurately. \
When referring to commands or code, use plain text without markdown formatting \
unless the user explicitly asks for it.";

// ---------------------------------------------------------------------------
// AskBuiltin
// ---------------------------------------------------------------------------

pub struct AskBuiltin;

impl SimpleCommand for AskBuiltin {
    fn get_content(name: &str, content_type: ContentType) -> Result<String, brush_core::Error> {
        let usage = format!("usage: {name} [--fresh|--no-transcript] [prompt...]\n");
        match content_type {
            ContentType::ShortUsage | ContentType::DetailedHelp | ContentType::ManPage => Ok(usage),
            ContentType::ShortDescription => {
                Ok(format!("{name} - invoke the configured AI model\n"))
            }
        }
    }

    fn execute<I: Iterator<Item = S>, S: AsRef<str>>(
        context: ExecutionContext<'_>,
        mut args: I,
    ) -> Result<ExecutionResult, brush_core::Error> {
        // The first element is the command name itself; skip it.
        args.next();
        Ok(run_ask(context, args))
    }
}

fn run_ask(
    context: ExecutionContext<'_>,
    args: impl Iterator<Item = impl AsRef<str>>,
) -> ExecutionResult {
    // --- argument parsing ---------------------------------------------------

    let mut fresh = false;
    let mut prompt_words: Vec<String> = Vec::new();

    for arg in args {
        match arg.as_ref() {
            "--fresh" | "--no-transcript" => fresh = true,
            other if other.starts_with('-') => {
                writeln!(
                    context.stderr(),
                    "ask: unknown flag {other:?}\nusage: ask [--fresh|--no-transcript] [prompt...]"
                )
                .ok();
                return invalid_usage();
            }
            other => prompt_words.push(other.to_owned()),
        }
    }

    // --- stdin --------------------------------------------------------------

    // Read piped stdin when it is not a TTY.  We use the real stdin here
    // because brush-core does not expose a way to read the process's stdin
    // from inside a builtin's execute method.
    let piped_stdin = {
        let stdin = std::io::stdin();
        if !stdin.is_terminal() {
            let mut buf = String::new();
            stdin.lock().read_to_string(&mut buf).unwrap_or(0);
            let trimmed = buf.trim_end_matches('\n').to_owned();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        } else {
            None
        }
    };

    // --- assemble prompt ----------------------------------------------------

    let prompt = match (prompt_words.is_empty(), &piped_stdin) {
        (true, None) => {
            writeln!(
                context.stderr(),
                "ask: no prompt provided\nusage: ask [--fresh|--no-transcript] [prompt...]"
            )
            .ok();
            return invalid_usage();
        }
        (true, Some(stdin)) => stdin.clone(),
        (false, None) => prompt_words.join(" "),
        (false, Some(stdin)) => format!("{}\n{}", prompt_words.join(" "), stdin),
    };

    // --- build messages -----------------------------------------------------

    let transcript_context: Option<String> = if fresh {
        None
    } else {
        let transcript = clank_transcript::global();
        let locked = transcript.lock().unwrap_or_else(|e| e.into_inner());
        if locked.is_empty() {
            None
        } else {
            Some(
                locked
                    .entries()
                    .map(|e| e.display_plain())
                    .collect::<Vec<_>>()
                    .join("\n"),
            )
        }
    };

    let user_content = match &transcript_context {
        Some(ctx) => format!("{ctx}\n\n---\n\n{prompt}"),
        None => prompt.clone(),
    };

    let messages = vec![
        Message {
            role: Role::System,
            content: SYSTEM_PROMPT.to_owned(),
        },
        Message {
            role: Role::User,
            content: user_content,
        },
    ];

    // --- provider call ------------------------------------------------------

    let http = Arc::new(NativeHttpClient::new());
    let provider = match provider_from_config(http) {
        Ok(p) => p,
        Err(ProviderError::NotConfigured(msg)) => {
            writeln!(context.stderr(), "ask: {msg}").ok();
            return invalid_usage();
        }
        Err(e) => {
            writeln!(context.stderr(), "ask: {e}").ok();
            return remote_error();
        }
    };

    // Run the async provider call on a dedicated OS thread with its own
    // single-thread tokio runtime.  This is the only safe way to drive async
    // code from a synchronous builtin context: brush-core already uses
    // `Handle::current().block_on(...)` internally, so `block_in_place` +
    // `block_on` on the same thread causes a deadlock rather than a panic.
    // A fresh thread with a fresh runtime has no such entanglement.
    let result = {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("provider runtime should build");
            let _ = tx.send(rt.block_on(provider.complete(&messages)));
        });
        rx.recv().unwrap_or_else(|_| {
            Err(clank_provider::ProviderError::Transport(
                "provider thread panicked".into(),
            ))
        })
    };

    // --- handle response ----------------------------------------------------

    match result {
        Ok(response) => {
            writeln!(context.stdout(), "{response}").ok();
            // Store the response for run_statement to record after the Command
            // entry.  Direct push here would record AiResponse before Command
            // because run_statement records Command only after execute returns.
            *pending_response().lock().unwrap_or_else(|e| e.into_inner()) = Some(response);
            ExecutionResult::success()
        }
        Err(ProviderError::NotConfigured(msg)) => {
            writeln!(context.stderr(), "ask: {msg}").ok();
            invalid_usage()
        }
        Err(ProviderError::Status(401)) => {
            writeln!(
                context.stderr(),
                "ask: authentication failed (check api key)"
            )
            .ok();
            invalid_usage()
        }
        Err(e) => {
            writeln!(context.stderr(), "ask: {e}").ok();
            remote_error()
        }
    }
}

// ---------------------------------------------------------------------------
// Exit code helpers
// ---------------------------------------------------------------------------

fn invalid_usage() -> ExecutionResult {
    ExecutionResult::from(brush_core::results::ExecutionExitCode::InvalidUsage)
}

fn remote_error() -> ExecutionResult {
    ExecutionResult::from(brush_core::results::ExecutionExitCode::Custom(4))
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

/// Returns a brush-core `Registration` for the `ask` builtin.
///
/// Called by `clank_core::default_options()` to register the builtin at
/// shell construction time.
pub fn ask_registration() -> Registration {
    simple_builtin::<AskBuiltin>()
}
