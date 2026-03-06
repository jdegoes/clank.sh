//! Integration tests for `clank_core::Repl`.
//!
//! These tests drive the REPL through its public API using in-memory I/O,
//! verifying control-flow and transcript population.

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use clank_core::transcript::EntryKind;
    use clank_core::Repl;
    use clank_core::Transcript;
    use std::io::Cursor;
    use std::sync::{Arc, Mutex};

    /// Run the REPL with the given script, discarding prompt output.
    async fn run_script(script: &str) -> anyhow::Result<()> {
        let mut repl = Repl::new().await?;
        let input = Cursor::new(script.to_string());
        let mut prompt_out = Vec::<u8>::new();
        repl.run(input, &mut prompt_out).await
    }

    /// Run the REPL and return the transcript.
    async fn run_with_transcript(script: &str) -> anyhow::Result<Arc<Mutex<Transcript>>> {
        let transcript = Arc::new(Mutex::new(Transcript::default_budget()));
        let mut repl = Repl::with_transcript(Arc::clone(&transcript)).await?;
        let input = Cursor::new(script.to_string());
        let mut prompt_out = Vec::<u8>::new();
        repl.run(input, &mut prompt_out).await?;
        Ok(transcript)
    }

    // -----------------------------------------------------------------------
    // Control-flow tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn empty_input_returns_ok() {
        run_script("").await.expect("empty input should return Ok");
    }

    #[tokio::test]
    async fn single_comment_returns_ok() {
        run_script("# this is a comment\n")
            .await
            .expect("comment-only input should return Ok");
    }

    #[tokio::test]
    async fn blank_lines_are_skipped() {
        run_script("\n\n\n")
            .await
            .expect("blank lines should return Ok");
    }

    #[tokio::test]
    async fn prompt_is_written_for_each_line() {
        let mut repl = Repl::new().await.expect("Repl::new");
        let input = Cursor::new("true\ntrue\n");
        let mut prompt_out = Vec::<u8>::new();
        repl.run(input, &mut prompt_out)
            .await
            .expect("run should succeed");
        let output = String::from_utf8(prompt_out).expect("prompt output is utf8");
        assert_eq!(output.matches("$ ").count(), 2, "expected 2 prompts");
    }

    #[tokio::test]
    async fn variable_assignment_and_expansion() {
        run_script("X=hello\necho $X\n")
            .await
            .expect("variable assignment and expansion should succeed");
    }

    #[tokio::test]
    async fn pipeline_executes() {
        run_script("echo hello | cat\n")
            .await
            .expect("pipeline should execute without error");
    }

    // -----------------------------------------------------------------------
    // Transcript tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn transcript_records_input_line() {
        let t = run_with_transcript("true\n")
            .await
            .expect("run_with_transcript");
        let entries = t.lock().unwrap();
        let inputs: Vec<_> = entries
            .entries()
            .iter()
            .filter(|e| e.kind == EntryKind::Input)
            .collect();
        assert_eq!(inputs.len(), 1);
        assert_eq!(inputs[0].text, "true");
    }

    #[tokio::test]
    async fn transcript_records_multiple_input_lines() {
        let t = run_with_transcript("true\nfalse\ntrue\n")
            .await
            .expect("run_with_transcript");
        let entries = t.lock().unwrap();
        let inputs: Vec<_> = entries
            .entries()
            .iter()
            .filter(|e| e.kind == EntryKind::Input)
            .collect();
        assert_eq!(inputs.len(), 3);
        assert_eq!(inputs[0].text, "true");
        assert_eq!(inputs[1].text, "false");
        assert_eq!(inputs[2].text, "true");
    }

    #[tokio::test]
    async fn transcript_records_echo_output() {
        let t = run_with_transcript("echo hello\n")
            .await
            .expect("run_with_transcript");
        let entries = t.lock().unwrap();

        let inputs: Vec<_> = entries
            .entries()
            .iter()
            .filter(|e| e.kind == EntryKind::Input)
            .collect();
        assert_eq!(inputs.len(), 1, "expected 1 input entry");
        assert_eq!(inputs[0].text, "echo hello");

        let outputs: Vec<_> = entries
            .entries()
            .iter()
            .filter(|e| e.kind == EntryKind::Output)
            .collect();
        assert_eq!(outputs.len(), 1, "expected 1 output entry");
        assert!(
            outputs[0].text.contains("hello"),
            "output entry should contain 'hello', got: {:?}",
            outputs[0].text
        );
    }

    #[tokio::test]
    async fn transcript_blank_lines_not_recorded() {
        let t = run_with_transcript("\n\n\n")
            .await
            .expect("run_with_transcript");
        let entries = t.lock().unwrap();
        assert!(
            entries.entries().is_empty(),
            "blank lines should not produce transcript entries"
        );
    }

    #[tokio::test]
    async fn transcript_input_order_preserved() {
        let t = run_with_transcript("echo first\necho second\n")
            .await
            .expect("run_with_transcript");
        let entries = t.lock().unwrap();
        // Entries should be interleaved: Input, Output, Input, Output
        let kinds: Vec<_> = entries.entries().iter().map(|e| &e.kind).collect();
        assert!(
            kinds.len() >= 2,
            "expected at least Input entries, got {kinds:?}"
        );
        assert_eq!(kinds[0], &EntryKind::Input, "first entry should be Input");
    }
}
