//! Output capture via OS pipe + background drain thread.
//!
//! Before each command, [`capture_stdout`] / [`capture_stderr`] create an OS
//! pipe. The write end is installed as fd 1/fd 2 on the `ExecutionParameters`.
//! A background thread drains the read end, forwarding bytes to the real
//! process stdout/stderr so output remains visible, while also collecting them
//! into a buffer. After `run_string` drops the `ExecutionParameters` (closing
//! the write end), [`CaptureHandle::join`] retrieves the captured text.

use std::io::{Read, Write};

use std::thread;

/// A handle to an in-flight capture operation.
///
/// Created by [`capture_stdout`] / [`capture_stderr`]. The `PipeWriter`
/// returned alongside this handle must be installed as fd 1/fd 2 on the
/// `ExecutionParameters` and will be dropped when those params go out of
/// scope, signalling EOF to the drain thread.
///
/// Call [`CaptureHandle::join`] after dropping the `ExecutionParameters` to
/// retrieve the captured text.
pub struct CaptureHandle {
    thread: thread::JoinHandle<String>,
}

impl CaptureHandle {
    /// Wait for the drain thread to finish and return the captured text.
    ///
    /// Must be called **after** the `PipeWriter` has been dropped (i.e. after
    /// `ExecutionParameters` goes out of scope). Otherwise the thread will
    /// block indefinitely waiting for EOF on the pipe.
    pub fn join(self) -> String {
        self.thread.join().unwrap_or_default()
    }
}

/// Create a capture pair for stdout.
///
/// Returns `(PipeWriter, CaptureHandle)`. Install the `PipeWriter` as fd 1
/// on the `ExecutionParameters`, drop the params after `run_string`, then
/// call `handle.join()`.
pub fn capture_stdout() -> std::io::Result<(std::io::PipeWriter, CaptureHandle)> {
    make_capture_pair(PassthroughTarget::Stdout)
}

/// Create a capture pair for stderr.
pub fn capture_stderr() -> std::io::Result<(std::io::PipeWriter, CaptureHandle)> {
    make_capture_pair(PassthroughTarget::Stderr)
}

// ---------------------------------------------------------------------------
// Internal
// ---------------------------------------------------------------------------

enum PassthroughTarget {
    Stdout,
    Stderr,
}

fn make_capture_pair(
    target: PassthroughTarget,
) -> std::io::Result<(std::io::PipeWriter, CaptureHandle)> {
    let (mut reader, writer) = std::io::pipe()?;

    let thread = thread::spawn(move || {
        let mut buf = [0u8; 4096];
        let mut captured = Vec::new();

        loop {
            match reader.read(&mut buf) {
                Ok(0) => break, // EOF — write end was dropped
                Ok(n) => {
                    captured.extend_from_slice(&buf[..n]);
                    // Forward to the real stream (best-effort).
                    match target {
                        PassthroughTarget::Stdout => {
                            let _ = std::io::stdout().write_all(&buf[..n]);
                        }
                        PassthroughTarget::Stderr => {
                            let _ = std::io::stderr().write_all(&buf[..n]);
                        }
                    }
                }
                Err(_) => break,
            }
        }

        String::from_utf8_lossy(&captured).into_owned()
    });

    Ok((writer, CaptureHandle { thread }))
}
