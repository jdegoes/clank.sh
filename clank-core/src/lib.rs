//! Core shell logic for clank.sh.
//!
//! On native targets this wraps [`brush_core::Shell`] to provide a minimal
//! read-eval-print loop. On `wasm32-wasip2` the shell interpreter is not yet
//! implemented (see the open issue for the WASM process model).

pub mod repl;
pub mod transcript;

#[cfg(not(target_arch = "wasm32"))]
pub(crate) mod tee;

pub use repl::Repl;
pub use transcript::{Entry, EntryKind, Transcript};
