/// Public surface for integration tests.
///
/// The `clank` binary crate exposes its process adapters via this lib target
/// so that integration tests in `tests/` can import them without subprocess
/// spawning.
pub mod processes;
