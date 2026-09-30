use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiProtocol {
    AnthropicMessages,
    OpenAiChat,
    OpenAiResponses,
    GeminiNative,
}

impl AiProtocol {
    pub fn from_str(s: &str) -> Option<Self> {
        let lowered = s.to_ascii_lowercase();
        let cleaned = lowered.trim();
        match cleaned {
            "anthropic" | "anthropic_messages" | "anthropic/messages" | "claude" => {
                Some(Self::AnthropicMessages)
            }
            "openai_chat" | "openai-chat" | "chat_completions" | "chat" | "openai" => {
                Some(Self::OpenAiChat)
            }
            "openai_responses" | "openai-responses" | "responses" | "codex" => {
                Some(Self::OpenAiResponses)
            }
            "gemini" | "gemini_native" | "gemini-native" => Some(Self::GeminiNative),
            _ => None,
        }
    }

    pub fn from_api_format(s: &str) -> Option<Self> {
        Self::from_str(s)
    }
}

/// Unified Tool Definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedTool {
    pub name: String,
    pub description: Option<String>,
    pub parameters: Value,
}

/// A Tool Call issued by the model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

/// Result of a tool call executed on the client side.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedToolResult {
    pub tool_use_id: String,
    pub content: String,
    pub is_error: bool,
}

/// A block of content inside a message.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum UnifiedContentBlock {
    Text { text: String },
    Thinking { thinking: String, signature: Option<String> },
    ToolCall(UnifiedToolCall),
    ToolResult(UnifiedToolResult),
}

/// A message in the conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedMessage {
    pub role: String, // "system", "user", "assistant", "tool"
    pub content: Vec<UnifiedContentBlock>,
}

impl UnifiedMessage {
    pub fn text_content(&self) -> String {
        let mut out = String::new();
        for block in &self.content {
            if let UnifiedContentBlock::Text { text } = block {
                out.push_str(text);
            }
        }
        out
    }
}

/// Normalized LLM Request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedRequest {
    pub model: String,
    pub system: Option<String>,
    pub messages: Vec<UnifiedMessage>,
    pub tools: Vec<UnifiedTool>,
    pub stream: bool,
    pub max_tokens: Option<u64>,
    pub temperature: Option<f64>,
    pub top_p: Option<f64>,
    pub thinking_budget_tokens: Option<u64>,
}
