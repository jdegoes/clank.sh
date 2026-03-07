---
title: "Transcript not passed to model — fix and test gaps"
date: 2026-03-07
author: agent
issue: dev-docs/issues/open/transcript-not-passed-to-model.md
research: []
designs: []
---

# Plan: Transcript not passed to model — fix and test gaps

## Root cause

`OpenRouterProvider` and `OpenAiCompatProvider` send the system prompt as a top-level `system`
field in the JSON body, via the shared `ChatRequest` wire struct in `provider/wire.rs`. This is
an Anthropic-specific API extension. The OpenAI `/v1/chat/completions` wire format — which both
OpenRouter and OpenAI-compat speak — does not support a top-level `system` field. It is silently
ignored by the server. The system prompt must instead be injected as the first element of the
`messages` array with `role: "system"`.

As a result, every `ask` invocation via OpenRouter or any OpenAI-compatible server sends no
system prompt — no transcript, no environment context, nothing. The model responds as if it
has no knowledge of the session.

The Anthropic provider is unaffected: it uses a separate wire struct that correctly passes
`system` at the top level, as the Anthropic Messages API specifies.

## Fix

### Task F1 — Fix OpenRouter and OpenAI-compat providers to inject system prompt as a message

In `provider/wire.rs`, remove the top-level `system` field from `ChatRequest`. Instead, in
`OpenRouterProvider::complete` and `OpenAiCompatProvider::complete`, prepend a
`{"role": "system", "content": <system_prompt>}` message to the `messages` array when
`system_prompt` is non-empty. This matches the OpenAI chat completions specification.

Concretely:

```rust
// Before building messages, prepend system if non-empty.
let mut wire_messages: Vec<ChatMessage<'_>> = Vec::new();
if !request.system_prompt.is_empty() {
    wire_messages.push(ChatMessage { role: "system", content: &request.system_prompt });
}
wire_messages.extend(request.messages.iter().map(|m| ChatMessage { ... }));
```

Remove `system: &'a str` from `ChatRequest` and remove it from the `json!()` call sites.

### Task F2 — Fix the two test gaps from the issue

**Gap 1 — Add a transcript capture test for registered subprocess commands**

Add a test to `crates/clank-shell/tests/transcript_capture.rs` that:
1. Runs `ls /` (a registered subprocess command that goes through the temp-file capture path).
2. Reads the transcript.
3. Asserts that an `Output` entry exists whose text contains expected output (e.g. a directory
   name known to exist under `/`).

This closes the gap where only `echo` (a Brush builtin bypassing the capture path) was tested.

**Gap 2 — Add an end-to-end test verifying transcript reaches the model request**

Add a test to `crates/clank/tests/processes.rs` that:
1. Creates a `ClankShell` with a shared transcript and a `MockHttpClient`.
2. Runs a command via `shell.run_line(...)` to populate the transcript.
3. Runs `AskProcess` with that transcript and mock HTTP client.
4. Inspects the captured `MockHttpClient` request body.
5. Asserts that the prior command's output appears in the request's system prompt or messages.

---

## Acceptance criteria

1. `ask "question"` after `ls -la` sends the `ls` output to the model in the system prompt
   when using OpenRouter or any OpenAI-compatible provider.
2. The new Gap 1 test passes: `ls /` output appears in an `Output` transcript entry.
3. The new Gap 2 test passes: prior transcript content appears in the outgoing HTTP request
   body when `AskProcess` runs.
4. All existing tests continue to pass.
5. `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check` pass.

---

## Implementation notes

- The `ChatRequest.system` field removal affects both OpenRouter and OpenAI-compat since they
  share the `wire.rs` struct. Both must be updated together in a single task.
- The Anthropic provider uses its own `ApiRequest` struct (not `ChatRequest`) — it is
  unaffected by this change.
- The existing `test_openrouter_builds_correct_request` test currently asserts the body
  contains a `system` field — this assertion must be updated to instead verify the system
  content appears as the first message with `role: "system"`.
- Similarly for `test_openai_compat_omits_openrouter_headers` and any other test that
  currently inspects the `system` field in the OpenRouter/OpenAI-compat request body.

---

## Note on how this bug evaded the test suite

The existing `test_openrouter_builds_correct_request` test inspects the serialised HTTP
request body produced by `OpenRouterProvider`. It should have caught this. It did not because
it was written to assert the presence of `system` as a top-level JSON field — which validated
the implementation, not the specification. A test that asserts what the code *does* will pass
even when the code is wrong; only a test that asserts what the API *requires* will catch a
spec deviation.

**The general principle this establishes for all provider wire format tests:**

Provider request body tests must be written against the API contract, not against the
current implementation. Concretely this means:

1. The test comment must cite the relevant section of the external API specification (e.g.
   "OpenAI chat completions spec: system prompt is `messages[0]` with `role: system`").
2. The assertion must verify the structural position and field names required by the spec,
   not merely that some field is present.
3. When a field is present in the code but absent from the spec, the test should assert
   it is *absent* from the serialised output — not just ignore it.

**A second approach that would also have caught this** is an end-to-end test at the
`AskProcess` level (Gap 2 above) that checks the outgoing request body after a real
transcript has been populated. This is complementary to the provider unit tests: the provider
tests verify the wire format is correct for a given input; the process-level test verifies
the right input reaches the provider in the first place. Both layers are needed. Had Gap 2's
test existed, it would have observed an empty or missing system prompt in the request body
and failed immediately, regardless of which provider was at fault.
