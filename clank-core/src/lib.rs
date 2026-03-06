//! Core shell logic for clank.sh.
//!
//! On native targets this wraps [`brush_core::Shell`] to provide a minimal
//! read-eval-print loop. On `wasm32-wasip2` the shell interpreter is not yet
//! implemented (see the open issue for the WASM process model).

pub mod repl;

pub use repl::Repl;
