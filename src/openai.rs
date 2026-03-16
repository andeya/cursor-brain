//! OpenAI protocol: request/response types (ChatCompletionRequest, build_completion_response), SSE.
//! Used by server (parsing requests) and service (building responses). No I/O; pure types and formatting.

use crate::cursor::CompletionOutput;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct ChatCompletionRequest {
    pub model: Option<String>,
    pub messages: Vec<ChatMessage>,
    pub stream: Option<bool>,
    #[serde(default)]
    pub temperature: Option<f32>,
    #[serde(default)]
    pub max_tokens: Option<u32>,
    #[serde(default)]
    pub tools: Option<serde_json::Value>,
    #[serde(default)]
    pub tool_choice: Option<serde_json::Value>,
}

fn message_content_to_string(content: &Option<serde_json::Value>) -> String {
    match content {
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(serde_json::Value::Array(arr)) => arr
            .iter()
            .filter_map(|c| {
                let obj = c.as_object()?;
                if obj.get("type").and_then(|t| t.as_str()) == Some("text") {
                    obj.get("text").and_then(|t| t.as_str()).map(String::from)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

pub fn extract_user_message(messages: &[ChatMessage]) -> String {
    for m in messages.iter().rev() {
        if m.role.eq_ignore_ascii_case("user") {
            return message_content_to_string(&m.content);
        }
    }
    String::new()
}

pub fn format_messages_as_prompt(messages: &[ChatMessage]) -> String {
    let mut out: Vec<String> = Vec::new();
    for m in messages.iter() {
        let content = message_content_to_string(&m.content);
        if content.is_empty() {
            continue;
        }
        let role = m.role.to_lowercase();
        let block = match role.as_str() {
            "system" => format!("System:\n{}", content),
            "user" => format!("User:\n{}", content),
            "assistant" => format!("Assistant:\n{}", content),
            "tool" => format!("Tool result:\n{}", content),
            _ => format!("{}:\n{}", m.role, content),
        };
        out.push(block);
    }
    out.join("\n\n---\n\n")
}

#[derive(Debug, Serialize)]
pub struct ChatCompletionResponse {
    pub id: String,
    pub object: String,
    pub created: u64,
    pub model: String,
    pub choices: Vec<Choice>,
    pub usage: Usage,
}

#[derive(Debug, Serialize)]
pub struct Choice {
    pub index: u32,
    pub message: Message,
    pub finish_reason: String,
}

#[derive(Debug, Serialize)]
pub struct Message {
    pub role: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_content: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

/// forward_thinking: "off" | "content" | "reasoning_content"
pub fn build_completion_response(
    id: &str,
    model: &str,
    out: &CompletionOutput,
    forward_thinking: &str,
) -> ChatCompletionResponse {
    let (content, reasoning_content) = match forward_thinking {
        "reasoning_content" => (out.content.clone(), out.reasoning_content.clone()),
        "content" if !out.thinking_text.is_empty() => {
            let block = out
                .thinking_text
                .trim()
                .lines()
                .map(|l| format!("> {}", l))
                .collect::<Vec<_>>()
                .join("\n");
            (format!("> 💭 {}\n\n---\n\n{}", block, out.content), None)
        }
        _ => (out.content.clone(), None),
    };
    ChatCompletionResponse {
        id: id.to_string(),
        object: "chat.completion".to_string(),
        created: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        model: model.to_string(),
        choices: vec![Choice {
            index: 0,
            message: Message {
                role: "assistant".to_string(),
                content,
                reasoning_content,
            },
            finish_reason: out.finish_reason.clone(),
        }],
        usage: Usage {
            prompt_tokens: 0,
            completion_tokens: 0,
            total_tokens: 0,
        },
    }
}

#[derive(Debug, Serialize)]
pub struct StreamChunk {
    pub id: String,
    pub object: String,
    pub created: u64,
    pub model: String,
    pub choices: Vec<StreamChoice>,
}

#[derive(Debug, Serialize)]
pub struct StreamChoice {
    pub index: u32,
    pub delta: StreamDelta,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct StreamDelta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_content: Option<String>,
}

pub fn sse_chunk(
    id: &str,
    model: &str,
    content: Option<&str>,
    finish_reason: Option<&str>,
) -> String {
    let chunk = StreamChunk {
        id: id.to_string(),
        object: "chat.completion.chunk".to_string(),
        created: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        model: model.to_string(),
        choices: vec![StreamChoice {
            index: 0,
            delta: StreamDelta {
                content: content.map(String::from),
                reasoning_content: None,
            },
            finish_reason: finish_reason.map(String::from),
        }],
    };
    let json = serde_json::to_string(&chunk).unwrap_or_default();
    format!("data: {}\n\n", json)
}

/// SSE chunk with only reasoning_content (for forward_thinking=reasoning_content).
pub fn sse_chunk_reasoning(id: &str, model: &str, reasoning_content: &str) -> String {
    let chunk = StreamChunk {
        id: id.to_string(),
        object: "chat.completion.chunk".to_string(),
        created: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        model: model.to_string(),
        choices: vec![StreamChoice {
            index: 0,
            delta: StreamDelta {
                content: None,
                reasoning_content: Some(reasoning_content.to_string()),
            },
            finish_reason: None,
        }],
    };
    let json = serde_json::to_string(&chunk).unwrap_or_default();
    format!("data: {}\n\n", json)
}

pub fn sse_done() -> &'static str {
    "data: [DONE]\n\n"
}
