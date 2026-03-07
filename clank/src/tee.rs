/// Real-time output capture for shell commands.
///
/// Each command runs with its stdout and stderr redirected into OS pipes.
/// A background thread reads from each pipe, writes bytes to the real terminal
/// immediately (so the operator sees output as it arrives), and accumulates
/// them for recording in the transcript after the command finishes.
use std::io::{Read, Write};
use std::thread;

/// The result of capturing a single output stream during command execution.
///
/// Created by [`pipe_and_capture`]. The caller installs the returned
/// `std::io::PipeWriter` as the target file descriptor on the shell's
/// `ExecutionParameters`, then drops those params when the command finishes.
/// Dropping the params closes the write end of the pipe, which causes the
/// background thread to terminate. Call [`StreamCapture::collect`] to wait
/// for the thread and retrieve the accumulated text.
pub struct StreamCapture {
    thread: thread::JoinHandle<String>,
}

impl StreamCapture {
    /// Wait for the capture thread to finish and return everything written
    /// to the stream. Must be called after the corresponding `PipeWriter`
    /// has been dropped.
    pub fn collect(self) -> String {
        self.thread.join().unwrap_or_default()
    }
}

/// Create a pipe whose write end can be handed to a shell command and whose
/// read end is drained in the background, forwarding bytes to `real_out`
/// while accumulating them for later retrieval via [`StreamCapture::collect`].
pub fn pipe_and_capture(
    mut real_out: impl Write + Send + 'static,
) -> std::io::Result<(std::io::PipeWriter, StreamCapture)> {
    let (mut reader, writer) = std::io::pipe()?;

    let thread = thread::spawn(move || {
        let mut buf = [0u8; 4096];
        let mut captured: Vec<u8> = Vec::new();

        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    captured.extend_from_slice(&buf[..n]);
                    // Forward to the real terminal immediately so output
                    // appears in real-time rather than after the command ends.
                    let _ = real_out.write_all(&buf[..n]);
                    let _ = real_out.flush();
                }
            }
        }

        String::from_utf8_lossy(&captured).into_owned()
    });

    Ok((writer, StreamCapture { thread }))
}

/// Tee stdout: pipe the stream to both the real terminal and a capture buffer.
pub fn tee_stdout() -> std::io::Result<(std::io::PipeWriter, StreamCapture)> {
    pipe_and_capture(std::io::stdout())
}

/// Tee stderr: pipe the stream to both the real terminal and a capture buffer.
pub fn tee_stderr() -> std::io::Result<(std::io::PipeWriter, StreamCapture)> {
    pipe_and_capture(std::io::stderr())
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collect_returns_all_written_bytes() {
        let (mut writer, capture) = pipe_and_capture(std::io::sink()).expect("pipe failed");
        writer.write_all(b"clank output").expect("write failed");
        drop(writer);
        assert_eq!(capture.collect(), "clank output");
    }

    #[test]
    fn collect_returns_empty_when_nothing_written() {
        let (writer, capture) = pipe_and_capture(std::io::sink()).expect("pipe failed");
        drop(writer);
        assert!(capture.collect().is_empty());
    }

    #[test]
    fn multiple_writes_are_accumulated_in_order() {
        let (mut writer, capture) = pipe_and_capture(std::io::sink()).expect("pipe failed");
        writer.write_all(b"first ").expect("write failed");
        writer.write_all(b"second").expect("write failed");
        drop(writer);
        assert_eq!(capture.collect(), "first second");
    }

    #[test]
    fn tee_stdout_does_not_panic() {
        let (mut writer, capture) = tee_stdout().expect("pipe failed");
        writer.write_all(b"hello").expect("write failed");
        drop(writer);
        assert_eq!(capture.collect(), "hello");
    }

    #[test]
    fn tee_stderr_does_not_panic() {
        let (mut writer, capture) = tee_stderr().expect("pipe failed");
        writer.write_all(b"err").expect("write failed");
        drop(writer);
        assert_eq!(capture.collect(), "err");
    }
}
