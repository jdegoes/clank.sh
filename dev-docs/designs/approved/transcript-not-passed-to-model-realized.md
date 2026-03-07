---
title: "Transcript not passed to model — realized design"
date: 2026-03-07
author: agent
---

# Realized Design: Transcript not passed to model

## Root cause confirmed

`OpenRouterProvider` and `OpenAiCompatProvider` sent the system prompt as a top-level
`system` field in the JSON body via the shared `ChatRequest` wire struct. This is an
Anthropic-specific API extension. The OpenAI `/v1/chat/completions` wire format — which
both providers speak — ignores this field silently. The transcript, embedded in the system
prompt, was dropped from every `ask` request made through OpenRouter or any OpenAI-compatible
server.

## What was built

**`provider/wire.rs`**: Removed the `system: &'a str` field from `ChatRequest`. Added a
comment citing the OpenAI API specification URL.

**`provider/openrouter.rs`** and **`provider/openai_compat.rs`**: Both providers now prepend
a `ChatMessage { role: "system", content: &request.system_prompt }` as the first element of
the `messages` array when `system_prompt` is non-empty. The spec URL is cited in the
implementation comment at each site.

## Test gaps closed

**Gap 1** (`transcript_capture.rs`): Added `test_registered_command_output_captured_in_transcript`
— runs `ls /` (a registered subprocess command going through the temp-file capture path) and
asserts its output lands in the transcript as an `Output` entry. Previously only Brush builtins
(`echo`) were tested, which bypass the capture path.

**Gap 2** (`processes.rs`): Added `test_ask_process_sends_transcript_in_request` — pre-populates
the transcript with a known sentinel string, runs `AskProcess` with an OpenRouter config and
`MockHttpClient`, deserialises the captured HTTP request body, and asserts the sentinel appears
in `messages[0].content` with `role: "system"`.

## Key decisions

- The OpenAI wire format spec URL is cited at three locations: the `ChatRequest` struct
  doc-comment, the OpenRouter implementation, and the OpenAI-compat implementation. This
  ensures future maintainers cannot accidentally reintroduce the top-level `system` field.
- The new provider tests assert the *absence* of a top-level `system` field as well as the
  presence of the system content in `messages[0]` — both sides of the contract.
- The existing `test_openrouter_builds_correct_request` was updated to add these assertions.

## Lesson recorded

Provider wire format tests must be written against the API contract, not against the
implementation. A test that asserts what the code does will pass even when the code is wrong;
only a test that asserts what the API requires will catch a spec deviation. This principle is
documented in the plan and issue files.
