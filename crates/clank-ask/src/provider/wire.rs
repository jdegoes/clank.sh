/// Shared OpenAI chat completions request/response wire types.
///
/// Used by both `OpenRouterProvider` and `OpenAiCompatProvider`, which speak
/// the same `/v1/chat/completions` wire format with different base URLs and
/// authentication behaviour.
///
/// OpenAI wire format reference:
/// <https://platform.openai.com/docs/api-reference/chat/create>
///
/// The system prompt is NOT a top-level field in this format — that is an
/// Anthropic-specific extension. Instead, the system prompt is passed as the
/// first element of `messages` with `role: "system"`. Both providers that use
/// this struct must prepend it themselves when `system_prompt` is non-empty.
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Request types
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct ChatRequest<'a> {
    pub model: &'a str,
    pub max_tokens: u32,
    pub messages: Vec<ChatMessage<'a>>,
    pub stream: bool,
}

#[derive(Serialize)]
pub struct ChatMessage<'a> {
    pub role: &'a str,
    pub content: &'a str,
}

// ---------------------------------------------------------------------------
// Response types
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct ChatResponse {
    pub choices: Vec<Choice>,
}

#[derive(Deserialize)]
pub struct Choice {
    pub message: AssistantMessage,
}

#[derive(Deserialize)]
pub struct AssistantMessage {
    pub content: String,
}
